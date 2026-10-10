pub(crate) mod column;
pub(crate) mod host;
pub(crate) mod panel;
pub(crate) mod project_picker;
pub(crate) mod rail;
mod rename;
mod row_menu;
pub(crate) mod sessions;
mod tree_tip;
mod update;
mod usage;

use crate::desktop::Desktop;
use crate::desktop::chrome::{Column, Layout, Overlay, RowMenu, Screen, drag_area, id, state};
use crate::sidebar::project_picker::ProjectPicker;
use crate::sidebar::rename::Rename;
use crate::sidebar::tree_tip::TreeTip;
use crate::status::{self, Card};
use crate::git_ui::pull_requests;
use crate::util::{LOCAL_ICON, WORKTREE_ICON, basename};
use agents::Agents;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::collections::HashMap;
use std::time::Duration;
use theme::*;
use ui::{self, icon_button_sized};

fn rolled_state(cards: &[Card]) -> Option<ui::State> {
    status::roll_up(cards.iter().map(|c| c.status)).map(|(s, _)| state(s, 0, 0))
}

/// The cards in `tree`: those opened in it as a Local, else those whose folder it holds, as `tree_of` places folders.
pub(crate) fn in_tree(cards: Vec<Card>, tree: Option<&str>, tree_of: impl Fn(&str) -> Option<String>) -> Vec<Card> {
    cards.into_iter().filter(|c| if c.local.is_empty() { tree_of(&c.cwd).as_deref() == tree } else { Some(c.local.as_str()) == tree }).collect()
}

/// The column left of the page, listing the inbox or Automations.
pub(crate) fn column_shown(layout: Layout, screen: Screen) -> bool {
    layout != Layout::Focus && matches!(screen, Screen::Inbox | Screen::Automations)
}

/// How a project's row folds its checkout and worktrees.
struct ProjectRow {
    git: bool,
    open: bool,
    setting_up: bool,
}

impl ProjectRow {
    fn new(worktrees: Option<&[git::Worktree]>, collapsed: bool, setups: &HashMap<String, String>) -> Self {
        let git = worktrees.is_some_and(|w| !w.is_empty());
        let open = git && !collapsed;
        let setting_up = !open && worktrees.into_iter().flatten().any(|w| setting_up(setups, &w.path));
        Self { git, open, setting_up }
    }

    /// Folded, the row stands for all its trees; open, their rows do.
    fn selected(&self, current: Option<&str>) -> bool {
        current.is_some() && !self.open
    }
}

/// Whether a terminal is setting up `tree`; `setups` maps terminals to the worktree they set up.
fn setting_up(setups: &HashMap<String, String>, tree: &str) -> bool {
    setups.values().any(|t| t == tree)
}

/// A tree's branch; a detached one names its folder.
fn tree_place(w: &git::Worktree) -> String {
    if w.branch.is_empty() || w.branch == "detached" { basename(&w.path) } else { w.branch.clone() }
}

/// A worktree row's label and hover tip: its given name over its `tree_place`.
pub(crate) fn tree_label(w: &git::Worktree, agents: &Agents) -> (String, String) {
    let place = tree_place(w);
    match agents.given_name(&w.path) {
        Some(title) => (title.to_string(), place),
        None => (place.clone(), place),
    }
}

/// A tree row's label, showing `tip` on hover.
fn tree_name(tree: &str, label: String, tip: String) -> Stateful<Div> {
    div().id(id(format!("aside-tree-label:{tree}"))).truncate().child(label).tooltip(move |_, cx| cx.new(|_| TreeTip(tip.clone())).into()).tooltip_show_delay(Duration::from_millis(350))
}

#[derive(Clone)]
struct DragProject {
    path: String,
    ix: usize,
}

pub struct SidebarState {
    pub(crate) search: Entity<InputState>,
    /// Where a right-click opened the row menu; `None` drops it under its `···` button.
    pub(crate) menu_at: Option<Point<Pixels>>,
    pub(crate) picker: ProjectPicker,
    /// The right-hand panel shows; collapsed at launch.
    pub(crate) panel_open: bool,
    /// The worktree row being renamed.
    pub(crate) rename: Option<Rename>,
}

impl SidebarState {
    pub fn new(panel_open: bool, window: &mut Window, cx: &mut Context<Desktop>) -> (Self, Vec<Subscription>) {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search sessions…"));
        let (picker, picker_subs) = ProjectPicker::new(window, cx);
        let mut subs = vec![cx.subscribe(&search, |_, _, _: &InputEvent, cx| cx.notify())];
        subs.extend(picker_subs);
        (Self { search, menu_at: None, picker, panel_open, rename: None }, subs)
    }
}

impl Desktop {
    /// The expanded sidebar: projects with their worktrees.
    pub(crate) fn aside(&self, cx: &mut Context<Self>) -> Div {
        let unseen = crate::inbox::count(&self.agents);
        let top = drag_area(div())
            .h(px(42.))
            .flex()
            .flex_none()
            .items_center()
            .justify_end()
            .child(
                icon_button_sized("aside-bell", "bell", 28., TEXT_3)
                    .when(unseen > 0, |d| d.child(ui::count_badge(unseen).top(px(-3.)).right(px(-3.))))
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open_inbox(window, cx))),
            )
            .child(
                icon_button_sized("aside-collapse", "sidebar-collapse", 28., TEXT_3)
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.toggle_sidebar(&crate::actions::ToggleSidebar, window, cx))),
            );
        let search = ui::trigger_field("aside-search", "search", "Search", "⌘K")
            .mx(px(4.))
            .mb(px(6.))
            .h(px(32.))
            .rounded(px(10.))
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open(Overlay::Palette, window, cx)));
        let automations = ui::nav_row("aside-automations", "bolt", "Automations", self.screen == Screen::Automations)
            .mb(px(2.))
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open_automations(&crate::actions::OpenAutomations, window, cx)));
        let header = div()
            .pt(px(10.))
            .pb(px(4.))
            .pl(px(6.))
            .pr(px(10.))
            .flex()
            .items_center()
            .text_size(px(11.))
            .font_weight(FontWeight::BOLD)
            .text_color(TEXT_3)
            .child("Projects");
        let mut repos = Vec::new();
        for (i, p) in self.projects().into_iter().enumerate() {
            repos.push(self.project_block(i, &p, cx));
        }
        let body = div().id("aside-repos").flex_1().min_h_0().overflow_y_scroll().flex().flex_col().gap(px(1.)).children(repos);
        let aside = ui::side(div())
            .w(px(self.width(Column::Projects, 272.)))
            .flex_none()
            .h_full()
            .px(px(8.))
            .pb(px(10.))
            .flex()
            .flex_col()
            .child(top)
            .child(search)
            .children(self.agents.automations_offered().then_some(automations))
            .child(header)
            .child(body)
            .children(self.update_card(cx));
        self.resizable(aside, Column::Projects, cx)
    }

    fn tree_cards(&self, project: &str, tree: &str) -> Vec<Card> {
        in_tree(self.cards(project), Some(tree), |cwd| self.tree_of(cwd))
    }

    /// A Local's row, by its key: the main worktree's path for the project's own, whose `selects` is `None`, else its id. `tip` is the checkout's branch.
    fn local_row(&self, p: &str, key: &str, selects: Option<&String>, tip: &str, current: Option<&str>, cx: &mut Context<Self>) -> AnyElement {
        let menu = RowMenu::Local(key.to_string());
        let renames = self.agents.renames_offered(key);
        let mark = ui::indicator(id(format!("aside-spin:{key}")), rolled_state(&self.tree_cards(p, key)));
        let label = match self.sidebar.rename.as_ref().filter(|r| r.tree == key) {
            Some(r) => Input::new(&r.input).appearance(false).p_0().text_size(px(13.)).into_any_element(),
            None => tree_name(key, self.local_name(key), tip.to_string()).into_any_element(),
        };
        let trail = renames.then(|| ui::row_trail(None, vec![self.row_menu_button(key, menu.clone(), cx)], self.row_menu.as_ref() == Some(&menu)));
        let (target, selects, path) = (p.to_string(), selects.cloned(), key.to_string());
        ui::worktree_row(id(format!("aside-tree:{key}")), LOCAL_ICON, label, current == Some(key), mark)
            .when(selects.is_none(), |row| row.children(self.pr_chip(key)))
            .children(trail)
            .on_click(cx.listener(move |this, ev: &ClickEvent, window, cx| {
                if this.sidebar.renaming(&path) {
                    return;
                }
                if ev.click_count() > 1 && renames {
                    return this.start_rename(path.clone(), window, cx);
                }
                this.select_tree(target.clone(), selects.clone(), cx);
            }))
            .when(renames, |row| row.on_mouse_down(MouseButton::Right, Self::open_row_menu(menu, cx)))
            .into_any_element()
    }

    /// A tree's pull request chip, if it has one.
    fn pr_chip(&self, tree: &str) -> Option<impl IntoElement> {
        self.prs.get(tree).map(|pr| pull_requests::chip(id(format!("aside-pr:{tree}")), pr))
    }

    /// A project's row, then while it is open its checkout's and worktrees' rows.
    fn project_block(&self, i: usize, p: &str, cx: &mut Context<Self>) -> AnyElement {
        let current = self.place().filter(|_| self.screen == Screen::Sessions && self.project.as_deref() == Some(p));
        let kept = self.store.projects.iter().any(|k| k == p);
        let main = self.tree_of(p).unwrap_or_else(|| p.to_string());
        let trees = self.listed_trees(p).map(|w| self.removals.hide(self.creates.trees(p, &w)));
        let setups = self.creates.setups(&self.terminals.setups);
        let fold = ProjectRow::new(trees.as_deref(), self.store.collapsed.contains(p), &setups);
        let selected = fold.selected(current.as_deref());
        let ProjectRow { git, open, setting_up } = fold;
        let state = if open { None } else { rolled_state(&self.cards(p)).filter(|_| !setting_up) };
        let chevron = git.then(|| ui::chevron(("aside-chevron", i), open));
        let menu = RowMenu::Project(p.to_string());
        let mut buttons = Vec::new();
        if git {
            let target = p.to_string();
            buttons.push(
                icon_button_sized(id(format!("aside-plus:{p}")), "plus", 22., TEXT_3)
                    .rounded(px(6.))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        this.select_tree(target.clone(), None, cx);
                    }))
                    .into_any_element(),
            );
        }
        buttons.push(self.row_menu_button(p, menu.clone(), cx));
        let setup = setting_up.then(|| ui::busy(id(format!("aside-setup:{p}")), "Setting up…").into_any_element());
        let trail = ui::row_trail(setup, buttons, self.row_menu.as_ref() == Some(&menu));
        let target = p.to_string();
        let row = ui::repo_row(("aside-repo", i), chevron, &self.repo_name(p), selected, kept, state)
            .child(trail)
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                if git {
                    this.store.toggle(&target);
                    this.store.save();
                    cx.notify();
                } else {
                    this.select_tree(target.clone(), None, cx);
                }
            }))
            .on_mouse_down(MouseButton::Right, Self::open_row_menu(menu, cx))
            .when(kept, |row| row.on_drag(DragProject { path: p.to_string(), ix: i }, |_, _, _, cx| cx.new(|_| EmptyView)));
        let mut out = vec![row.into_any_element()];
        if open {
            let tip = trees.iter().flatten().find(|w| w.main).map(tree_place).unwrap_or_default();
            out.push(self.local_row(p, &main, None, &tip, current.as_deref(), cx));
            let locals: Vec<String> = self.agents.locals_in(p).map(|l| l.id.clone()).collect();
            for local in &locals {
                out.push(self.local_row(p, local, Some(local), &tip, current.as_deref(), cx));
            }
            let trees: Vec<(String, String, String)> = trees
                .iter()
                .flatten()
                .filter(|w| !w.main)
                .map(|w| {
                    let (label, tip) = tree_label(w, &self.agents);
                    (w.path.clone(), label, tip)
                })
                .collect();
            for (tree, label, tip) in &trees {
                let menu = RowMenu::Tree { project: p.to_string(), tree: tree.clone() };
                let mark = if self.creates.failed(tree) {
                    ui::indicator(id(format!("aside-failed:{tree}")), Some(ui::State::Failed))
                } else if self::setting_up(&setups, tree) {
                    Some(dot_spinner(id(format!("aside-setup:{tree}")), 11., TEXT_4).into_any_element())
                } else {
                    ui::indicator(id(format!("aside-spin:{tree}")), rolled_state(&self.tree_cards(p, tree)))
                };
                let trail = ui::row_trail(None, vec![self.row_menu_button(tree, menu.clone(), cx)], self.row_menu.as_ref() == Some(&menu));
                let (target, path) = (p.to_string(), tree.clone());
                let label = match self.sidebar.rename.as_ref().filter(|r| &r.tree == tree) {
                    Some(r) => Input::new(&r.input).appearance(false).p_0().text_size(px(13.)).into_any_element(),
                    None => tree_name(tree, label.clone(), tip.clone()).into_any_element(),
                };
                out.push(
                    ui::worktree_row(id(format!("aside-tree:{tree}")), WORKTREE_ICON, label, current.as_ref() == Some(tree), mark)
                        .children(self.pr_chip(tree))
                        .child(trail)
                        .on_click(cx.listener(move |this, ev: &ClickEvent, window, cx| {
                            // The rename input sits inside the row; its clicks must not restart or leave it.
                            if this.sidebar.renaming(&path) {
                                return;
                            }
                            if ev.click_count() > 1 && this.agents.names_offered() {
                                return this.start_rename(path.clone(), window, cx);
                            }
                            this.creates.show_progress(&path);
                            this.select_tree(target.clone(), Some(path.clone()), cx);
                        }))
                        .on_mouse_down(MouseButton::Right, Self::open_row_menu(menu, cx))
                        .into_any_element(),
                );
            }
        }
        let to = p.to_string();
        div()
            .flex()
            .flex_col()
            .gap(px(1.))
            .when(open, |d| d.pb(px(6.)))
            .children(out)
            .when(kept, |d| {
                d.drag_over::<DragProject>(move |s, d, _, _| if d.ix == i { s } else { s.shadow(ui::drop_line(d.ix < i)) }).on_drop(cx.listener(move |this, d: &DragProject, _, cx| {
                    this.store.move_project(&d.path, &to);
                    this.store.save();
                    cx.notify();
                }))
            })
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{ProjectRow, column_shown, in_tree, setting_up, tree_label};
    use crate::desktop::chrome::{Layout, Screen};
    use crate::status::{self, Card};
    use agents::{Agents, Summary};
    use std::collections::HashMap;

    fn card(id: &str, cwd: &str) -> Card {
        status::card(&Summary { id: id.into(), status: "idle".into(), attached: true, ..Default::default() }, cwd)
    }

    fn ids(cards: Vec<Card>) -> Vec<String> {
        cards.into_iter().map(|c| c.id).collect()
    }

    fn tree_of(cwd: &str) -> Option<String> {
        match cwd {
            "/p" | "/p/src" => Some("/p".into()),
            "/wt" => Some("/wt".into()),
            _ => None,
        }
    }

    #[test]
    fn a_trees_cards_are_those_whose_folder_it_holds() {
        let cards = vec![card("a", "/p"), card("b", "/wt"), card("c", "/p/src"), card("d", "/lost")];
        assert_eq!(ids(in_tree(cards, Some("/p"), tree_of)), vec!["a", "c"]);
    }

    #[test]
    fn a_locals_cards_are_those_opened_in_it_whatever_their_folder() {
        let local = |id: &str, local: &str| Card { local: local.into(), ..card(id, "/p") };
        let cards = vec![card("a", "/p"), local("b", "l1"), local("c", "l2")];
        assert_eq!(ids(in_tree(cards.clone(), Some("l1"), tree_of)), vec!["b"]);
        assert_eq!(ids(in_tree(cards, Some("/p"), tree_of)), vec!["a"]);
    }

    #[test]
    fn no_tree_holds_the_cards_outside_every_tree() {
        let cards = vec![card("a", "/p"), card("d", "/lost")];
        assert_eq!(ids(in_tree(cards, None, tree_of)), vec!["d"]);
    }

    fn tree(path: &str, main: bool) -> git::Worktree {
        git::Worktree { path: path.into(), branch: String::new(), main }
    }

    #[test]
    fn a_git_project_folds_even_with_only_its_checkout() {
        let none = HashMap::new();
        let fold = |trees: &[git::Worktree], collapsed| {
            let r = ProjectRow::new(Some(trees), collapsed, &none);
            (r.git, r.open)
        };
        let got = [fold(&[], false), fold(&[tree("/p", true)], false), fold(&[tree("/p", true), tree("/wt", false)], true)];
        assert_eq!(got, [(false, false), (true, true), (true, false)]);
    }

    #[test]
    fn a_folded_project_shows_its_worktrees_setting_up_and_an_open_one_leaves_it_to_their_rows() {
        let setups: HashMap<String, String> = [("term".to_string(), "/wt".to_string())].into();
        let trees = [tree("/p", true), tree("/wt", false)];
        let folded = ProjectRow::new(Some(&trees), true, &setups).setting_up;
        let open = ProjectRow::new(Some(&trees), false, &setups).setting_up;
        assert_eq!((folded, open, setting_up(&setups, "/wt"), setting_up(&setups, "/p")), (true, false, true, false));
    }

    #[test]
    fn a_folded_row_is_selected_on_any_of_its_trees_and_an_open_one_leaves_it_to_their_rows() {
        let trees = [tree("/p", true), tree("/wt", false)];
        let selected = |collapsed, current| ProjectRow::new(Some(&trees), collapsed, &HashMap::new()).selected(current);
        let got = [selected(true, Some("/wt")), selected(false, Some("/wt")), selected(false, Some("/p")), selected(true, None)];
        assert_eq!(got, [true, false, false, false]);
    }

    #[test]
    fn a_worktree_shows_its_name_over_its_branch_and_a_detached_one_its_folder() {
        let named = git::Worktree { path: "/wt/calm-otter".into(), branch: "fix-login".into(), main: false };
        let (mut name, none) = (Agents::default(), Agents::default());
        name.set_name("/wt/calm-otter", "Fix the login form");
        assert_eq!(tree_label(&named, &name), ("Fix the login form".into(), "fix-login".into()));
        assert_eq!(tree_label(&named, &none), ("fix-login".into(), "fix-login".into()));
        let detached = git::Worktree { branch: "detached".into(), ..named };
        assert_eq!(tree_label(&detached, &none), ("calm-otter".into(), "calm-otter".into()));
        assert_eq!(tree_label(&detached, &name), ("Fix the login form".into(), "calm-otter".into()));
    }

    #[test]
    fn the_left_column_lists_the_inbox_and_automations_outside_focus() {
        let got = [
            column_shown(Layout::Sidebars, Screen::Sessions),
            column_shown(Layout::Compact, Screen::Inbox),
            column_shown(Layout::Sidebars, Screen::Automations),
            column_shown(Layout::Focus, Screen::Inbox),
        ];
        assert_eq!(got, [false, true, true, false]);
    }
}
