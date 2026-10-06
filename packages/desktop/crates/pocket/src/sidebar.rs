pub(crate) mod column;
mod host;
pub(crate) mod project_picker;
pub(crate) mod rail;
mod rename;
mod row_menu;
pub(crate) mod sessions;
mod tree_tip;
mod usage;

use crate::desktop::Desktop;
use crate::desktop::chrome::{Column, Layout, Overlay, RowMenu, Screen, drag_area, id, state};
use crate::sidebar::project_picker::ProjectPicker;
use crate::sidebar::rename::Rename;
use crate::sidebar::tree_tip::TreeTip;
use crate::status::{self, Card};
use crate::git_ui::pull_requests;
use crate::util::basename;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::collections::HashMap;
use std::time::Duration;
use theme::*;
use ui::{self, icon_button_sized};

fn row_mark(key: &str, setting_up: bool, cards: &[Card]) -> Option<AnyElement> {
    if setting_up {
        Some(ui::busy(id(format!("aside-setup:{key}")), "Setting up…").into_any_element())
    } else {
        let rolled = status::roll_up(cards.iter().map(|c| c.status)).map(|(s, _)| state(s, 0, 0));
        ui::indicator(id(format!("aside-spin:{key}")), rolled)
    }
}

/// The cards whose folder lies in `tree`, as `tree_of` places folders.
pub(crate) fn in_tree(cards: Vec<Card>, tree: Option<&str>, tree_of: impl Fn(&str) -> Option<String>) -> Vec<Card> {
    cards.into_iter().filter(|c| tree_of(&c.cwd).as_deref() == tree).collect()
}

/// The Sessions, Explorer and Changes column; the inbox lists in it too.
pub(crate) fn column_shown(layout: Layout, screen: Screen, hidden: bool) -> bool {
    match layout {
        Layout::Sidebars => true,
        Layout::Compact => screen == Screen::Inbox || !hidden,
        Layout::Focus => false,
    }
}

/// How a project's row folds its worktrees.
struct ProjectRow {
    git: bool,
    branched: bool,
    open: bool,
    setting_up: bool,
}

impl ProjectRow {
    fn new(worktrees: Option<&[git::Worktree]>, collapsed: bool, setups: &HashMap<String, String>) -> Self {
        let git = worktrees.is_some_and(|w| !w.is_empty());
        let branched = worktrees.is_some_and(|w| w.iter().any(|w| !w.main));
        let open = branched && !collapsed;
        let setting_up = !open && worktrees.into_iter().flatten().any(|w| setting_up(setups, &w.path));
        Self { git, branched, open, setting_up }
    }

    /// Folded, the row stands for all its worktrees; open, for its main one.
    fn selected(&self, current: Option<&str>, main: &str) -> bool {
        current.is_some_and(|c| !self.open || c == main)
    }
}

/// Whether a terminal is setting up `tree`; `setups` maps terminals to the worktree they set up.
fn setting_up(setups: &HashMap<String, String>, tree: &str) -> bool {
    setups.values().any(|t| t == tree)
}

/// A worktree row's label and hover tip: its display name over its branch; a detached one names its folder.
fn tree_label(w: &git::Worktree, names: &HashMap<String, String>) -> (String, String) {
    let place = if w.branch.is_empty() || w.branch == "detached" { basename(&w.path) } else { w.branch.clone() };
    match names.get(&w.path).filter(|t| !t.is_empty()) {
        Some(title) => (title.clone(), place),
        None => (place.clone(), place),
    }
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
    /// Whether the user hid Compact's column.
    pub(crate) column_hidden: bool,
    /// The worktree row being renamed.
    pub(crate) rename: Option<Rename>,
}

impl SidebarState {
    pub fn new(window: &mut Window, cx: &mut Context<Desktop>) -> (Self, Vec<Subscription>) {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search sessions…"));
        let (picker, picker_subs) = ProjectPicker::new(window, cx);
        let mut subs = vec![cx.subscribe(&search, |_, _, _: &InputEvent, cx| cx.notify())];
        subs.extend(picker_subs);
        (Self { search, menu_at: None, picker, column_hidden: false, rename: None }, subs)
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
        let header = div()
            .pt(px(10.))
            .pb(px(4.))
            .px(px(10.))
            .flex()
            .items_center()
            .text_size(px(12.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(TEXT_3)
            .child(div().flex_1().child("Projects"))
            .child(
                icon_button_sized("aside-add", "plus", 22., TEXT_3)
                    .rounded(px(6.))
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open(Overlay::AddRepo, window, cx))),
            );
        let mut repos = Vec::new();
        for (i, p) in self.projects().into_iter().enumerate() {
            repos.push(self.project_block(i, &p, cx));
        }
        let body = div().id("aside-repos").flex_1().min_h_0().overflow_y_scroll().flex().flex_col().gap(px(1.)).children(repos);
        let foot = div()
            .pt(px(8.))
            .px(px(2.))
            .flex()
            .items_center()
            .gap(px(6.))
            .child(
                ui::button("aside-add-repo", ui::Variant::Glass, Some("plus"), "Add project")
                    .flex_1()
                    .h(px(36.))
                    .rounded(px(17.))
                    .justify_center()
                    .text_size(px(13.5))
                    .font_weight(FontWeight::SEMIBOLD)
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open(Overlay::AddRepo, window, cx))),
            )
            .child(
                ui::glass(icon_button_sized("aside-settings", "settings", 34., TEXT).rounded(px(17.)))
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open_settings(&crate::actions::OpenSettings, window, cx))),
            );
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
            .child(header)
            .child(body)
            .children(self.usage_card())
            .children(self.host_lines(cx))
            .child(foot);
        self.resizable(aside, Column::Projects, cx)
    }

    fn tree_cards(&self, project: &str, tree: &str) -> Vec<Card> {
        in_tree(self.cards(project), Some(tree), |cwd| self.tree_of(cwd))
    }

    /// A project's row, then its worktrees' rows while it is open.
    fn project_block(&self, i: usize, p: &str, cx: &mut Context<Self>) -> AnyElement {
        let current = self.cwd().filter(|_| self.screen == Screen::Sessions && self.project.as_deref() == Some(p));
        let kept = self.store.projects.iter().any(|k| k == p);
        let main = self.tree_of(p).unwrap_or_else(|| p.to_string());
        let trees = self.worktrees.get(p).map(|w| self.removals.hide(self.creates.trees(p, w)));
        let setups = self.creates.setups(&self.terminals.setups);
        let fold = ProjectRow::new(trees.as_deref(), self.store.collapsed.contains(p), &setups);
        let selected = fold.selected(current.as_deref(), &main);
        let ProjectRow { git, branched, open, setting_up } = fold;
        let cards = if open { self.tree_cards(p, &main) } else { self.cards(p) };
        let lead = if branched {
            let target = p.to_string();
            ui::chevron(("aside-chevron", i), open)
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    this.store.toggle(&target);
                    this.store.save();
                    cx.notify();
                }))
                .into_any_element()
        } else {
            div().w(px(14.)).flex_none().into_any_element()
        };
        let menu = RowMenu::Project(p.to_string());
        let mut buttons = Vec::new();
        if git {
            let target = p.to_string();
            buttons.push(
                icon_button_sized(id(format!("aside-plus:{p}")), "plus", 22., TEXT_3)
                    .rounded(px(6.))
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        cx.stop_propagation();
                        this.new_worktree_in(target.clone(), window, cx);
                    }))
                    .into_any_element(),
            );
        }
        buttons.push(self.row_menu_button(p, menu.clone(), cx));
        let trail = ui::row_trail(row_mark(p, setting_up, &cards), buttons, self.row_menu.as_ref() == Some(&menu));
        let target = p.to_string();
        let row = ui::repo_row(("aside-repo", i), lead, &self.repo_name(p), selected, kept)
            .child(trail)
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.select_tree(target.clone(), None, cx)))
            .on_mouse_down(MouseButton::Right, Self::open_row_menu(menu, cx))
            .when(kept, |row| row.on_drag(DragProject { path: p.to_string(), ix: i }, |_, _, _, cx| cx.new(|_| EmptyView)));
        let mut out = vec![row.into_any_element()];
        if open {
            let trees: Vec<(String, String, String)> = trees
                .iter()
                .flatten()
                .filter(|w| !w.main)
                .map(|w| {
                    let (label, tip) = tree_label(w, &self.agents.names);
                    (w.path.clone(), label, tip)
                })
                .collect();
            for (tree, label, tip) in &trees {
                let menu = RowMenu::Tree { project: p.to_string(), tree: tree.clone() };
                let mark = if self.creates.failed(tree) {
                    ui::indicator(id(format!("aside-failed:{tree}")), Some(ui::State::Failed))
                } else {
                    row_mark(tree, self::setting_up(&setups, tree), &self.tree_cards(p, tree))
                };
                let trail = ui::row_trail(mark, vec![self.row_menu_button(tree, menu.clone(), cx)], self.row_menu.as_ref() == Some(&menu));
                let (target, path) = (p.to_string(), tree.clone());
                let label = match self.sidebar.rename.as_ref().filter(|r| &r.tree == tree) {
                    Some(r) => Input::new(&r.input).appearance(false).p_0().text_size(px(13.5)).into_any_element(),
                    None => {
                        let tip = tip.clone();
                        div()
                            .id(id(format!("aside-tree-label:{tree}")))
                            .truncate()
                            .child(label.clone())
                            .tooltip(move |_, cx| cx.new(|_| TreeTip(tip.clone())).into())
                            .tooltip_show_delay(Duration::from_millis(350))
                            .into_any_element()
                    }
                };
                out.push(
                    ui::worktree_row(id(format!("aside-tree:{tree}")), label, current.as_ref() == Some(tree))
                        .children(self.prs.get(tree).map(|pr| pull_requests::chip(id(format!("aside-pr:{tree}")), pr)))
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
    use agents::Summary;
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
    fn no_tree_holds_the_cards_outside_every_tree() {
        let cards = vec![card("a", "/p"), card("d", "/lost")];
        assert_eq!(ids(in_tree(cards, None, tree_of)), vec!["d"]);
    }

    fn tree(path: &str, main: bool) -> git::Worktree {
        git::Worktree { path: path.into(), branch: String::new(), main }
    }

    #[test]
    fn a_project_folds_only_when_it_has_worktrees_besides_its_main() {
        let none = HashMap::new();
        let fold = |trees: &[git::Worktree], collapsed| {
            let r = ProjectRow::new(Some(trees), collapsed, &none);
            (r.git, r.branched, r.open)
        };
        let got = [fold(&[], false), fold(&[tree("/p", true)], false), fold(&[tree("/p", true), tree("/wt", false)], false), fold(&[tree("/p", true), tree("/wt", false)], true)];
        assert_eq!(got, [(false, false, false), (true, false, false), (true, true, true), (true, true, false)]);
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
    fn a_folded_row_is_selected_on_any_of_its_trees_and_an_open_one_on_its_main_only() {
        let trees = [tree("/p", true), tree("/wt", false)];
        let selected = |collapsed, current| ProjectRow::new(Some(&trees), collapsed, &HashMap::new()).selected(current, "/p");
        let got = [selected(true, Some("/wt")), selected(false, Some("/wt")), selected(false, Some("/p")), selected(true, None)];
        assert_eq!(got, [true, false, true, false]);
    }

    #[test]
    fn a_worktree_shows_its_name_over_its_branch_and_a_detached_one_its_folder() {
        let named = git::Worktree { path: "/wt/calm-otter".into(), branch: "fix-login".into(), main: false };
        let names: HashMap<String, String> = [("/wt/calm-otter".to_string(), "Fix the login form".to_string())].into();
        assert_eq!(tree_label(&named, &names), ("Fix the login form".into(), "fix-login".into()));
        assert_eq!(tree_label(&named, &HashMap::new()), ("fix-login".into(), "fix-login".into()));
        let detached = git::Worktree { branch: "detached".into(), ..named };
        assert_eq!(tree_label(&detached, &HashMap::new()), ("calm-otter".into(), "calm-otter".into()));
        assert_eq!(tree_label(&detached, &names), ("Fix the login form".into(), "calm-otter".into()));
    }

    #[test]
    fn compact_shows_the_column_unless_hidden_and_always_for_the_inbox() {
        let got = [
            column_shown(Layout::Sidebars, Screen::Sessions, true),
            column_shown(Layout::Compact, Screen::Sessions, false),
            column_shown(Layout::Compact, Screen::Sessions, true),
            column_shown(Layout::Compact, Screen::Inbox, true),
            column_shown(Layout::Focus, Screen::Sessions, false),
        ];
        assert_eq!(got, [true, true, false, true, false]);
    }
}
