use crate::desktop::Desktop;
use crate::desktop::chrome::Screen;
use crate::sidebar::aside::{Body, GroupBody, GroupFrame, PinnedFrame, PinnedItem, ProjectFrame, SectionsFrame, SidebarEvent, Tail, TreeFrame};
use crate::sidebar::filter::FIELD_H;
use crate::sidebar::fold::Fold;
use crate::sidebar::header::{HeaderFrame, ProjectsHeader};
use crate::sidebar::rename::rename_field;
use crate::sidebar::{ProjectRow, Sidebar, rolled_state, tree_label, tree_place};
use crate::spaces::{Space, space_of};
use gpui_kit::component::input::InputState;
use gpui_kit::*;
use std::time::Instant;
use store::sections::{Pin, Sort};

#[derive(Default)]
pub(crate) struct Organize {
    pub(crate) rename: Option<GroupRename>,
}

pub(crate) struct GroupRename {
    pub(crate) group: u32,
    pub(crate) input: Entity<InputState>,
    _sub: Subscription,
}

/// A section header's row, as tall as the filter's field so opening that doesn't push the list down.
pub(super) fn section_head() -> Div {
    div().pt(px(10.)).pb(px(4.)).pr(px(10.)).h(px(10. + FIELD_H + 4.)).flex().flex_none().items_center().gap(px(2.))
}

/// Where a project row sits, and so what dropping a project on it does.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Section {
    Pinned,
    Group(u32),
    Other,
}

/// What a project block lists: `only` holds the tree keys the filter lets through, `None` unfiltered.
pub(crate) struct Block {
    pub(crate) ix: usize,
    pub(crate) project: String,
    pub(crate) only: Option<Vec<String>>,
    /// The project's own name matches the filter.
    pub(crate) named: bool,
}

pub(crate) enum Item {
    Project(Block),
    Tree { project: String, key: String },
}

/// The sidebar's projects, sectioned.
pub(crate) struct Listed {
    pub(crate) pinned: Vec<Item>,
    pub(crate) groups: Vec<(u32, Vec<Block>)>,
    pub(crate) other: Vec<Block>,
}

/// The kind of row a tree has under its project.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Kind {
    /// The project's own Local, keyed by its main worktree's path; listed only while it has a terminal or tab open.
    Own,
    Local,
    Worktree,
}

/// A tree row under a project: its key, label and hover tip.
pub(crate) struct TreeRow {
    pub(crate) key: String,
    pub(crate) label: String,
    pub(crate) tip: String,
    pub(crate) kind: Kind,
    /// Its place among its project's rows.
    pub(crate) ix: usize,
}

pub(super) fn matches(text: &str, query: &str) -> bool {
    text.to_lowercase().contains(query)
}

/// The keys of `trees` (key, label) a project named `name` lists under `query`: all when the name matches, else those whose label does; `None` when nothing matches.
fn shown(query: &str, name: &str, trees: &[(String, String)]) -> Option<Vec<String>> {
    let all = query.is_empty() || matches(name, query);
    let keys: Vec<String> = trees.iter().filter(|(_, label)| all || matches(label, query)).map(|(k, _)| k.clone()).collect();
    (all || !keys.is_empty()).then_some(keys)
}

/// `recent` is a project's last activity.
fn sort_projects(projects: &mut [String], sort: Sort, name: impl Fn(&str) -> String, recent: impl Fn(&str) -> i64) {
    match sort {
        Sort::Manual => {}
        Sort::Name => projects.sort_by_cached_key(|p| name(p).to_lowercase()),
        Sort::Recent => projects.sort_by_cached_key(|p| std::cmp::Reverse(recent(p))),
    }
}

pub(crate) fn emoji_mark(e: String) -> Div {
    div().text_size(px(13.)).line_height(px(16.)).child(e)
}

impl Desktop {
    /// `p`'s trees, less those being deleted, plus those being made; `None` until git lists them.
    pub(super) fn trees_shown(&self, p: &str, cx: &App) -> Option<Vec<git::Worktree>> {
        self.listed_trees(p).map(|w| self.removals.hide(self.creates.read(cx).trees(p, &w)))
    }

    /// The rows under `p`: its own Local while in use, the Locals added to it, then its worktrees, in the order they were dragged to.
    pub(super) fn tree_rows(&self, p: &str, trees: &[git::Worktree], cx: &App) -> Vec<TreeRow> {
        if trees.is_empty() {
            return vec![];
        }
        let main = self.tree_of(p, cx).unwrap_or_else(|| p.to_string());
        let tip = trees.iter().find(|w| w.main).map(tree_place).unwrap_or_default();
        let mut rows = Vec::new();
        if self.in_use(&main, cx) {
            rows.push(TreeRow { label: self.local_name(&main), key: main, tip: tip.clone(), kind: Kind::Own, ix: 0 });
        }
        rows.extend(self.agents.locals_in(p).map(|l| TreeRow { key: l.id.clone(), label: self.local_name(&l.id), tip: tip.clone(), kind: Kind::Local, ix: 0 }));
        rows.extend(trees.iter().filter(|w| !w.main).map(|w| {
            let (label, tip) = tree_label(w, &self.agents);
            TreeRow { key: w.path.clone(), label, tip, kind: Kind::Worktree, ix: 0 }
        }));
        let rows = self.store.sections.order_trees(p, rows, |r| r.key.as_str());
        rows.into_iter().enumerate().map(|(ix, r)| TreeRow { ix, ..r }).collect()
    }

    pub(super) fn tree_pinned(&self, p: &str, key: &str) -> bool {
        self.store.sections.is_pinned(&Pin::Tree { project: p.into(), tree: key.into() })
    }

    /// The projects in their sections, sorted, with what `query` leaves of them.
    pub(super) fn listed(&self, query: &str, cx: &App) -> Listed {
        let space = self.space();
        let mut projects = self.space_projects(&space);
        let sections = &self.store.sections;
        sort_projects(&mut projects, sections.sort, |p| self.repo_name(p), |p| self.cards(p, cx).iter().map(|c| c.at).max().unwrap_or(0));
        let arranged = sections.arrange(&projects, space.host(), |p| self.store.projects.iter().any(|k| k == p));
        let block = |ix: usize| {
            let p = &projects[ix];
            let only = if query.is_empty() {
                None
            } else {
                let trees: Vec<(String, String)> = self
                    .tree_rows(p, &self.trees_shown(p, cx).unwrap_or_default(), cx)
                    .into_iter()
                    .filter(|r| !self.tree_pinned(p, &r.key))
                    .map(|r| (r.key, r.label))
                    .collect();
                Some(shown(query, &self.repo_name(p), &trees)?)
            };
            Some(Block { ix, project: p.clone(), only, named: !query.is_empty() && matches(&self.repo_name(p), query) })
        };
        let pinned = arranged
            .pinned
            .iter()
            .filter_map(|pin| match pin {
                Pin::Project(p) => block(projects.iter().position(|x| x == p)?).map(Item::Project),
                Pin::Tree { project, tree } => {
                    if space_of(project) != space {
                        return None;
                    }
                    // Kept shut until its project's trees are listed, so it springs open then.
                    let Some(trees) = self.trees_shown(project, cx) else {
                        return query.is_empty().then(|| Item::Tree { project: project.clone(), key: tree.clone() });
                    };
                    let row = self.tree_rows(project, &trees, cx).into_iter().find(|r| &r.key == tree)?;
                    let shows = query.is_empty() || matches(&self.repo_name(project), query) || matches(&row.label, query);
                    shows.then(|| Item::Tree { project: project.clone(), key: row.key })
                }
            })
            .collect();
        let groups = arranged.groups.into_iter().map(|(id, ixs)| (id, ixs.into_iter().filter_map(block).collect())).collect();
        Listed { pinned, groups, other: arranged.other.into_iter().filter_map(block).collect() }
    }

    pub(super) fn projects_header(&self, cx: &mut Context<Self>) -> Entity<ProjectsHeader> {
        let frame = HeaderFrame { sort: self.store.sections.sort, host: self.space_host_name(&self.space()), locked: self.locked_space().is_some() };
        let header = self.sidebar.read(cx).header.clone();
        header.update(cx, |h, cx| h.set(frame, cx));
        header
    }

    /// Adds a group, holding `project` if given, and names it in place.
    pub(super) fn new_group(&mut self, project: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        let host = self.space().host().map(str::to_string);
        let sections = &mut self.store.sections;
        let id = sections.add_group("New group", host.as_deref());
        if let Some(p) = project {
            sections.set_group(&p, Some(id));
        }
        self.store.save();
        self.start_group_rename(id, window, cx);
    }

    pub(super) fn start_group_rename(&mut self, group: u32, window: &mut Window, cx: &mut Context<Self>) {
        let Some(name) = self.store.sections.group(group).map(|g| g.name.clone()) else { return };
        self.row_menu = None;
        self.sidebar.update(cx, |s, cx| s.begin_group_rename(group, name, window, cx));
        cx.notify();
    }

    /// Moves the dragged project `path` into `to`.
    pub(super) fn drop_into(&mut self, path: &str, to: Section) {
        let sections = &mut self.store.sections;
        match to {
            Section::Pinned => sections.pin(Pin::Project(path.to_string())),
            Section::Group(g) => sections.set_group(path, Some(g)),
            Section::Other => sections.set_group(path, None),
        }
        self.store.save();
    }

    pub(super) fn body_frame(&self, query: &str, space: &Space, cx: &mut Context<Self>) -> Body {
        let filtering = !query.is_empty();
        let listed = self.listed(query, cx);
        let sections = &self.store.sections;
        let nothing = listed.pinned.is_empty() && listed.other.is_empty() && listed.groups.iter().all(|(_, blocks)| blocks.is_empty());
        if !filtering && nothing {
            return Body::Empty(self.empty_space_name(space));
        }
        let pinned_open = filtering || !sections.pins_folded;
        let pinned = (!listed.pinned.is_empty()).then(|| {
            let items = if self.sidebar.read(cx).closing.drawn(&Fold::Pinned, pinned_open) {
                listed
                    .pinned
                    .iter()
                    .map(|item| match item {
                        Item::Project(b) => PinnedItem::Project(self.project_frame(b.ix, &b.project, b.only.as_deref(), Section::Pinned, cx)),
                        Item::Tree { project, key } => PinnedItem::Tree { key: key.clone(), row: self.pinned_tree_frame(project, key, cx).map(Box::new) },
                    })
                    .collect()
            } else {
                vec![]
            };
            PinnedFrame { open: pinned_open, items }
        });
        let mut groups = Vec::new();
        for (id, blocks) in &listed.groups {
            let Some(group) = sections.group(*id) else { continue };
            if filtering && blocks.is_empty() {
                continue;
            }
            let open = filtering || !group.folded;
            let body = match (self.sidebar.read(cx).closing.drawn(&Fold::Group(*id), open), blocks.is_empty()) {
                (false, _) => GroupBody::Hidden,
                (true, true) => GroupBody::Hint,
                (true, false) => {
                    GroupBody::Blocks(blocks.iter().map(|b| self.project_frame(b.ix, &b.project, b.only.as_deref(), Section::Group(*id), cx)).collect())
                }
            };
            groups.push(GroupFrame { id: *id, name: group.name.clone(), icon: group.icon.clone(), open, body });
        }
        let above = pinned.is_some() || !groups.is_empty();
        let tail = if filtering && listed.other.is_empty() {
            if above { Tail::None } else { Tail::NoMatch(self.sidebar.read(cx).header.read(cx).filter.input.read(cx).value().trim().to_string()) }
        } else {
            let blocks = listed.other.iter().map(|b| self.project_frame(b.ix, &b.project, b.only.as_deref(), Section::Other, cx)).collect();
            Tail::Other { blocks, headed: above || !listed.groups.is_empty() }
        };
        Body::Sections(SectionsFrame { pinned_open, pinned, groups, tail })
    }

    /// A pinned tree's row, naming its project; `None` once the tree is gone.
    fn pinned_tree_frame(&self, p: &str, key: &str, cx: &App) -> Option<TreeFrame> {
        let trees = self.trees_shown(p, cx)?;
        let row = self.tree_rows(p, &trees, cx).into_iter().find(|r| r.key == key)?;
        let current = self.place(cx).filter(|_| self.screen == Screen::Sessions && self.project.as_deref() == Some(p));
        Some(self.tree_frame(p, &row, current.as_deref(), &self.creates.read(cx).setups(&self.terminals.read(cx).setups), Some(self.repo_name(p)), cx))
    }

    /// A project's row, then while it is open its checkout's and worktrees' rows, less those pinned.
    /// `only` holds the trees the filter lets through, unfolding it; `section` is where it sits, so where a project dropped on it goes.
    fn project_frame(&self, ix: usize, p: &str, only: Option<&[String]>, section: Section, cx: &App) -> ProjectFrame {
        let current = self.place(cx).filter(|_| self.screen == Screen::Sessions && self.project.as_deref() == Some(p));
        let trees = self.trees_shown(p, cx);
        let setups = self.creates.read(cx).setups(&self.terminals.read(cx).setups);
        let fold = ProjectRow::new(trees.as_deref(), only.is_none() && self.store.collapsed.contains(p), &setups);
        let selected = fold.selected(current.as_deref());
        let ProjectRow { git, open, setting_up } = fold;
        let state = if open { None } else { self.calm(p, rolled_state(&self.cards(p, cx)).filter(|_| !setting_up)) };
        let name = self.repo_name(p);
        let offline = hosts::split(p).0.is_some_and(|h| !self.fleet.hosts.links.online(h));
        let rows = if self.sidebar.read(cx).closing.drawn(&Fold::Project(p.to_string()), open) {
            self.tree_rows(p, trees.as_deref().unwrap_or_default(), cx)
                .iter()
                .filter(|r| only.is_none_or(|o| o.contains(&r.key)) && !self.tree_pinned(p, &r.key))
                .map(|row| self.tree_frame(p, row, current.as_deref(), &setups, None, cx))
                .collect()
        } else {
            vec![]
        };
        ProjectFrame {
            ix,
            path: p.to_string(),
            mark: self.project_mark(p, ui::mark_letter(&name)),
            name,
            git,
            open,
            setting_up,
            selected,
            state,
            offline,
            lit: self.sidebar.read(cx).lit(cx, p, None),
            flashed: self.spaces.flashed(p, Instant::now()),
            section,
            manual: self.store.sections.sort == Sort::Manual,
            rows,
        }
    }
}

impl Sidebar {
    pub(crate) fn begin_group_rename(&mut self, group: u32, name: String, window: &mut Window, cx: &mut Context<Self>) {
        let (input, sub) = rename_field(name, window, cx, Self::finish_group_rename);
        self.organize.rename = Some(GroupRename { group, input, _sub: sub });
        cx.notify();
    }

    fn finish_group_rename(&mut self, cx: &mut Context<Self>) {
        if let Some(r) = self.organize.rename.take() {
            let name = r.input.read(cx).value().to_string();
            cx.emit(SidebarEvent::GroupRenamed { group: r.group, name });
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::{shown, sort_projects};
    use store::sections::Sort;

    fn trees() -> Vec<(String, String)> {
        [("/p", "Local"), ("/wt/a", "fix-login"), ("/wt/b", "feat-search")].map(|(k, l)| (k.to_string(), l.to_string())).to_vec()
    }

    #[test]
    fn a_matching_project_lists_all_its_trees_and_otherwise_only_the_matching_ones() {
        assert_eq!(shown("pock", "coding-pocket", &trees()), Some(vec!["/p".into(), "/wt/a".into(), "/wt/b".into()]));
        assert_eq!(shown("login", "coding-pocket", &trees()), Some(vec!["/wt/a".into()]));
        assert_eq!(shown("nothing", "coding-pocket", &trees()), None);
    }

    #[test]
    fn a_project_without_trees_shows_when_its_name_matches() {
        assert_eq!(shown("pock", "coding-pocket", &[]), Some(vec![]));
        assert_eq!(shown("x", "coding-pocket", &[]), None);
    }

    #[test]
    fn sorting_by_name_ignores_case_and_by_recent_puts_the_latest_first() {
        let mut ps = vec!["/b".to_string(), "/a".into(), "/c".into()];
        let name = |p: &str| match p {
            "/a" => "beta".to_string(),
            "/b" => "Alpha".into(),
            _ => "gamma".into(),
        };
        sort_projects(&mut ps, Sort::Name, name, |_| 0);
        assert_eq!(ps, ["/b", "/a", "/c"]);
        sort_projects(&mut ps, Sort::Recent, name, |p| if p == "/c" { 9 } else { 1 });
        assert_eq!(ps, ["/c", "/b", "/a"]);
        sort_projects(&mut ps, Sort::Manual, name, |_| 0);
        assert_eq!(ps, ["/c", "/b", "/a"]);
    }
}
