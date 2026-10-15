use crate::desktop::Desktop;
use crate::desktop::chrome::{RowMenu, Screen};
use crate::sidebar::filter::FIELD_H;
use crate::sidebar::fold::{self, Fold};
use crate::sidebar::rename::rename_field;
use crate::sidebar::{DragProject, tree_label, tree_place};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use store::sections::{Group, Pin, Sort};
use theme::*;
use ui::{self, icon_button_sized};

/// The Projects header's organize menu, and the group being renamed.
#[derive(Default)]
pub(crate) struct Organize {
    pub(crate) menu: bool,
    pub(crate) rename: Option<GroupRename>,
}

pub(crate) struct GroupRename {
    pub(crate) group: u32,
    pub(crate) input: Entity<InputState>,
    _sub: Subscription,
}

/// Where a project row sits, and so what dropping a project on it does.
#[derive(Clone, Copy)]
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
}

fn matches(text: &str, query: &str) -> bool {
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

/// A section's rows under its head, springing to their height as it opens and shuts; `group_box`'s gap moves inside, so a shut section keeps its height.
fn section_body(id: impl Into<ElementId>, open: bool, rows: Vec<(AnyElement, f32)>) -> impl IntoElement {
    let height = if rows.is_empty() { 0. } else { 2. + fold::stacked(rows.iter().map(|r| r.1), 2.) };
    let body = div().flex().flex_col().gap(px(2.)).when(!rows.is_empty(), |d| d.pt(px(2.))).children(rows.into_iter().map(|r| r.0));
    fold::springing(id, open, height, body)
}

pub(crate) fn group_mark(g: &Group) -> Option<Div> {
    g.icon.clone().map(|e| div().text_size(px(13.)).line_height(px(16.)).child(e))
}

impl Desktop {
    /// `p`'s trees, less those being deleted, plus those being made; `None` until git lists them.
    pub(super) fn trees_shown(&self, p: &str) -> Option<Vec<git::Worktree>> {
        self.listed_trees(p).map(|w| self.removals.hide(self.creates.trees(p, &w)))
    }

    /// The rows under `p`: its own Local while in use, the Locals added to it, then its worktrees.
    pub(super) fn tree_rows(&self, p: &str, trees: &[git::Worktree]) -> Vec<TreeRow> {
        if trees.is_empty() {
            return vec![];
        }
        let main = self.tree_of(p).unwrap_or_else(|| p.to_string());
        let tip = trees.iter().find(|w| w.main).map(tree_place).unwrap_or_default();
        let mut rows = Vec::new();
        if self.in_use(&main) {
            rows.push(TreeRow { label: self.local_name(&main), key: main, tip: tip.clone(), kind: Kind::Own });
        }
        rows.extend(self.agents.locals_in(p).map(|l| TreeRow { key: l.id.clone(), label: self.local_name(&l.id), tip: tip.clone(), kind: Kind::Local }));
        rows.extend(trees.iter().filter(|w| !w.main).map(|w| {
            let (label, tip) = tree_label(w, &self.agents);
            TreeRow { key: w.path.clone(), label, tip, kind: Kind::Worktree }
        }));
        rows
    }

    pub(super) fn tree_pinned(&self, p: &str, key: &str) -> bool {
        self.store.sections.is_pinned(&Pin::Tree { project: p.into(), tree: key.into() })
    }

    /// The projects in their sections, sorted, with what `query` leaves of them.
    pub(super) fn listed(&self, query: &str) -> Listed {
        let mut projects = self.projects();
        let sections = &self.store.sections;
        sort_projects(&mut projects, sections.sort, |p| self.repo_name(p), |p| self.cards(p).iter().map(|c| c.at).max().unwrap_or(0));
        let arranged = sections.arrange(&projects, |p| self.store.projects.iter().any(|k| k == p));
        let block = |ix: usize| {
            let p = &projects[ix];
            let only = if query.is_empty() {
                None
            } else {
                let trees: Vec<(String, String)> = self.tree_rows(p, &self.trees_shown(p).unwrap_or_default()).into_iter().filter(|r| !self.tree_pinned(p, &r.key)).map(|r| (r.key, r.label)).collect();
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
                    let row = self.tree_rows(project, &self.trees_shown(project)?).into_iter().find(|r| &r.key == tree)?;
                    let shows = query.is_empty() || matches(&self.repo_name(project), query) || matches(&row.label, query);
                    shows.then(|| Item::Tree { project: project.clone(), key: row.key })
                }
            })
            .collect();
        let groups = arranged.groups.into_iter().map(|(id, ixs)| (id, ixs.into_iter().filter_map(block).collect())).collect();
        Listed { pinned, groups, other: arranged.other.into_iter().filter_map(block).collect() }
    }

    /// The Projects header: its name and search button, or the filter's field while it's open, then its menus.
    pub(super) fn projects_header(&self, cx: &mut Context<Self>) -> Div {
        // As tall as the filter's field, so opening it doesn't push the list down.
        let header = div().pt(px(10.)).pb(px(4.)).pr(px(10.)).h(px(10. + FIELD_H + 4.)).flex().flex_none().items_center().gap(px(2.));
        let header = if self.sidebar.filter.open {
            header.child(self.filter_field(cx))
        } else {
            let search = icon_button_sized("aside-filter", "search", 22., TEXT_3).rounded(px(6.)).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open_filter(window, cx)));
            header.pl(px(6.)).child(div().flex_1().text_size(px(11.)).font_weight(FontWeight::BOLD).text_color(TEXT_3).child("PROJECTS")).child(search)
        };
        header
            .child(self.organize_menu(cx))
            .child(self.add_menu(cx))
    }

    /// The header's sort button and its menu: new group, the sort, folding every project.
    fn organize_menu(&self, cx: &mut Context<Self>) -> Div {
        let open = self.sidebar.organize.menu;
        let button = icon_button_sized("aside-organize", "arrow-down-wide-narrow", 22., TEXT_3).rounded(px(6.)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
            this.close_menus();
            this.sidebar.organize.menu = !open;
            cx.notify();
        }));
        div().relative().child(button).when(open, |d| {
            let new = ui::menu_row("aside-new-group", "folder-plus", "New group", None).on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                this.sidebar.organize.menu = false;
                this.new_group(None, window, cx);
            }));
            let sort = self.store.sections.sort;
            let sorts = [(Sort::Manual, "Manual"), (Sort::Name, "Name"), (Sort::Recent, "Recent activity")].map(|(s, label)| {
                ui::menu_pick(SharedString::from(format!("aside-sort-{label}")), div(), label, s == sort).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.sidebar.organize.menu = false;
                    this.store.sections.sort = s;
                    this.store.save();
                    cx.notify();
                }))
            });
            let fold = |id: &'static str, label: &'static str, fold: bool| {
                ui::menu_row(id, if fold { "chevron-right" } else { "chevron-down" }, label, None).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.sidebar.organize.menu = false;
                    if fold {
                        for p in this.store.projects.iter().filter(|p| !this.store.collapsed.contains(*p)) {
                            this.sidebar.closing.start(Fold::Project(p.clone()));
                        }
                    }
                    this.store.collapsed = if fold { this.store.projects.iter().cloned().collect() } else { Default::default() };
                    this.store.save();
                    cx.notify();
                }))
            };
            let menu = ui::pop(div().id("aside-organize-menu"))
                .w(px(220.))
                .p(px(6.))
                .flex()
                .flex_col()
                .occlude()
                .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                    this.sidebar.organize.menu = false;
                    cx.notify();
                }))
                .child(new)
                .child(ui::menu_divider())
                .child(ui::menu_label("Sort projects"))
                .children(sorts)
                .child(ui::menu_divider())
                .child(fold("aside-collapse-all", "Collapse all projects", true))
                .child(fold("aside-expand-all", "Expand all projects", false));
            d.child(ui::dropdown_right(26., ui::menu_in("aside-organize-menu-in", menu)))
        })
    }

    /// Adds a group, holding `project` if given, and names it in place.
    pub(super) fn new_group(&mut self, project: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        let sections = &mut self.store.sections;
        let id = sections.add_group("New group");
        if let Some(p) = project {
            sections.set_group(&p, Some(id));
        }
        self.store.save();
        self.start_group_rename(id, window, cx);
    }

    pub(super) fn start_group_rename(&mut self, group: u32, window: &mut Window, cx: &mut Context<Self>) {
        let Some(name) = self.store.sections.group(group).map(|g| g.name.clone()) else { return };
        let (input, sub) = rename_field(name, window, cx, Self::save_group_rename);
        self.row_menu = None;
        self.sidebar.organize.rename = Some(GroupRename { group, input, _sub: sub });
        cx.notify();
    }

    fn save_group_rename(&mut self, cx: &mut Context<Self>) {
        if let Some(r) = self.sidebar.organize.rename.take() {
            self.store.sections.rename_group(r.group, &r.input.read(cx).value());
            self.store.save();
        }
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

    /// Takes a project dropped anywhere on `el` into `to`.
    pub(super) fn drops_into<E: InteractiveElement + StatefulInteractiveElement>(el: E, to: Section, cx: &mut Context<Self>) -> E {
        el.drag_over::<DragProject>(|s, _, _, _| s.bg(FILL_2)).on_drop(cx.listener(move |this, d: &DragProject, _, cx| {
            this.drop_into(&d.path, to);
            cx.notify();
        }))
    }

    pub(super) fn pin_button(&self, key: &str, pin: Pin, cx: &mut Context<Self>) -> AnyElement {
        let pinned = self.store.sections.is_pinned(&pin);
        icon_button_sized(SharedString::from(format!("aside-pin:{key}")), "pin", 22., if pinned { TEXT } else { TEXT_3 })
            .rounded(px(6.))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                this.store.sections.toggle_pin(pin.clone());
                this.store.save();
                cx.notify();
            }))
            .into_any_element()
    }

    /// Pinned rows, each group, then the ungrouped projects; while filtering, only what matches.
    pub(super) fn sections(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let query = self.sidebar.filter.query(cx);
        let filtering = !query.is_empty();
        let listed = self.listed(&query);
        let sections = &self.store.sections;
        let mut out = Vec::new();
        if !listed.pinned.is_empty() {
            let open = filtering || !sections.pins_folded;
            let head = ui::section_head("aside-pinned", icon("pin", 14., TEXT), "Pinned", open, None, false).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.store.sections.pins_folded = !this.store.sections.pins_folded;
                if this.store.sections.pins_folded {
                    this.sidebar.closing.start(Fold::Pinned);
                }
                this.store.save();
                cx.notify();
            }));
            let rows: Vec<(AnyElement, f32)> = if self.sidebar.closing.drawn(&Fold::Pinned, open) {
                listed
                    .pinned
                    .iter()
                    .filter_map(|item| match item {
                        Item::Project(b) => Some(self.project_block(b.ix, &b.project, b.only.as_deref(), Section::Pinned, cx)),
                        Item::Tree { project, key } => self.pinned_tree(project, key, cx).map(|row| (row, ui::SIDEBAR_ROW_H)),
                    })
                    .collect()
            } else {
                vec![]
            };
            let body = section_body("aside-pinned-fold", open, rows);
            out.push(Self::drops_into(ui::group_box().id("aside-pinned-box").gap_0().child(head).child(body), Section::Pinned, cx).into_any_element());
        } else if !filtering && cx.has_active_drag() {
            let zone = div()
                .id("aside-pin-drop")
                .mb(px(8.))
                .h(px(36.))
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .gap(px(6.))
                .rounded(px(10.))
                .border(px(1.))
                .border_dashed()
                .border_color(TEXT_5)
                .text_size(px(12.5))
                .text_color(TEXT_3)
                .child(icon("pin", 13., TEXT_3))
                .child("Drop to pin");
            out.push(Self::drops_into(zone, Section::Pinned, cx).into_any_element());
        }
        for (id, blocks) in &listed.groups {
            let Some(group) = sections.group(*id) else { continue };
            if filtering && blocks.is_empty() {
                continue;
            }
            let open = filtering || !group.folded;
            let id = *id;
            let name = match self.sidebar.organize.rename.as_ref().filter(|r| r.group == id) {
                Some(r) => Input::new(&r.input).appearance(false).p_0().text_size(px(14.)).into_any_element(),
                None => group.name.clone().into_any_element(),
            };
            let menu = RowMenu::Group(id);
            let more = self.row_menu_button(&format!("group:{id}"), menu.clone(), cx);
            let head = ui::section_head(("aside-group", id as usize), div().children(group_mark(group)), name, open, Some(more), self.row_menu.as_ref() == Some(&menu))
                .on_click(cx.listener(move |this, ev: &ClickEvent, window, cx| {
                    if this.sidebar.organize.rename.as_ref().is_some_and(|r| r.group == id) {
                        return;
                    }
                    // A double-click's second click undoes the fold its first made.
                    this.store.sections.toggle_group(id);
                    if this.store.sections.group(id).is_some_and(|g| g.folded) {
                        this.sidebar.closing.start(Fold::Group(id));
                    }
                    this.store.save();
                    if ev.click_count() > 1 {
                        return this.start_group_rename(id, window, cx);
                    }
                    cx.notify();
                }))
                .on_mouse_down(MouseButton::Right, Self::open_row_menu(menu, cx));
            let rows: Vec<(AnyElement, f32)> = match (self.sidebar.closing.drawn(&Fold::Group(id), open), blocks.is_empty()) {
                (false, _) => vec![],
                (true, true) => vec![(div().h(px(ui::SIDEBAR_ROW_H)).px(px(10.)).flex().items_center().text_size(px(12.5)).text_color(TEXT_4).child("Drag projects here").into_any_element(), ui::SIDEBAR_ROW_H)],
                (true, false) => blocks.iter().map(|b| self.project_block(b.ix, &b.project, b.only.as_deref(), Section::Group(id), cx)).collect(),
            };
            let body = section_body(("aside-group-fold", id as usize), open, rows);
            out.push(Self::drops_into(ui::group_box().id(("aside-group-box", id as usize)).gap_0().child(head).child(body), Section::Group(id), cx).into_any_element());
        }
        if filtering && listed.other.is_empty() {
            if out.is_empty() {
                out.push(div().px(px(10.)).py(px(12.)).text_size(px(12.5)).text_color(TEXT_3).child(format!("No projects or worktrees match “{}”.", self.sidebar.filter.input.read(cx).value().trim())).into_any_element());
            }
            return out;
        }
        let rows: Vec<AnyElement> = listed.other.iter().map(|b| self.project_block(b.ix, &b.project, b.only.as_deref(), Section::Other, cx).0).collect();
        let other = div().id("aside-other").flex().flex_col().gap(px(1.)).when((!out.is_empty() || !sections.groups.is_empty()) && (!rows.is_empty() || cx.has_active_drag()), |d| {
            d.child(div().h(px(28.)).px(px(10.)).flex().flex_none().items_center().gap(px(8.)).text_size(px(12.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT_3).child("Other").child(ui::group_count(rows.len())))
        });
        out.push(Self::drops_into(other.children(rows), Section::Other, cx).into_any_element());
        out
    }

    /// A pinned tree's row, naming its project; `None` once the tree is gone.
    fn pinned_tree(&self, p: &str, key: &str, cx: &mut Context<Self>) -> Option<AnyElement> {
        let trees = self.trees_shown(p)?;
        let row = self.tree_rows(p, &trees).into_iter().find(|r| r.key == key)?;
        let current = self.place().filter(|_| self.screen == Screen::Sessions && self.project.as_deref() == Some(p));
        Some(self.place_row(p, &row, current.as_deref(), &self.creates.setups(&self.terminals.setups), Some(self.repo_name(p)), cx))
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
