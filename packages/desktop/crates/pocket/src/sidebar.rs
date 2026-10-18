pub(crate) mod column;
mod filter;
mod fold;
pub(crate) mod host;
pub(crate) mod panel;
pub(crate) mod project_picker;
pub(crate) mod rail;
mod rename;
mod row_menu;
pub(crate) mod sections;
pub(crate) mod sessions;
mod tree_card;
mod update;
mod usage;

use crate::desktop::Desktop;
use crate::desktop::chrome::{Column, Layout, Overlay, RowMenu, Screen, drag_area, id, state};
use crate::sidebar::project_picker::ProjectPicker;
use crate::sidebar::rename::Rename;
use crate::sidebar::filter::Filter;
use crate::sidebar::fold::{Closing, Fold};
use crate::sidebar::sections::{Kind, Organize, Section, TreeRow};
use crate::sidebar::tree_card::Hover;
use crate::status::{self, Card};
use crate::git_ui::pull_requests;
use crate::util::{LOCAL_ICON, WORKTREE_ICON, ago_long, basename, now_ms};
use agents::Agents;
use store::sections::{Pin, Sort};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::collections::HashMap;
use std::time::Instant;
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

/// A host's project shows a globe, with its link's dot.
fn host_mark(link: Option<&hosts::link::Link>) -> Div {
    div().flex().flex_none().items_center().gap(px(3.)).child(icon("globe", 12., TEXT_3)).child(ui::dot(6., crate::settings::ssh_hosts::link_color(link)))
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
fn tree_label(w: &git::Worktree, agents: &Agents) -> (String, String) {
    let place = tree_place(w);
    match agents.given_name(&w.path) {
        Some(title) => (title.to_string(), place),
        None => (place.clone(), place),
    }
}

/// The colour a working tree's label sweeps from; `None` while it isn't working, or motion is reduced.
fn sweep_base(state: Option<ui::State>, selected: bool, reduce_motion: bool) -> Option<Token> {
    (state == Some(ui::State::Working) && !reduce_motion).then_some(if selected { TEXT } else { TEXT_BODY })
}

/// A tree row's label, sweeping from `sweeps` while its tree works.
fn tree_name(tree: &str, label: String, sweeps: Option<Token>) -> AnyElement {
    match sweeps {
        Some(base) => ui::sweep(id(format!("aside-sweep:{tree}")), label.into(), base).into_any_element(),
        None => label.into_any_element(),
    }
}

/// `label` bright and semibold while its tree is done, which lasts until it is viewed.
/// Styled outside the marquee, which measures the label in the style it inherits.
fn done_name(label: AnyElement, state: Option<ui::State>) -> AnyElement {
    match state {
        Some(ui::State::Done(..)) => div().text_color(TEXT).font_weight(FontWeight::SEMIBOLD).child(label).into_any_element(),
        _ => label,
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
    /// The Projects + menu shows.
    pub(crate) add_menu: bool,
    /// The right-hand panel shows; collapsed at launch.
    pub(crate) panel_open: bool,
    /// The worktree row being renamed.
    pub(crate) rename: Option<Rename>,
    /// Which row's hover card shows.
    pub(crate) hover: Hover,
    pub(crate) filter: Filter,
    pub(crate) organize: Organize,
    pub(crate) closing: Closing,
}

impl SidebarState {
    pub fn new(panel_open: bool, window: &mut Window, cx: &mut Context<Desktop>) -> (Self, Vec<Subscription>) {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search sessions…"));
        let (picker, picker_subs) = ProjectPicker::new(window, cx);
        let (filter, filter_subs) = Filter::new(window, cx);
        let mut subs = vec![cx.subscribe(&search, |_, _, _: &InputEvent, cx| cx.notify())];
        subs.extend(picker_subs);
        subs.extend(filter_subs);
        (Self { search, menu_at: None, picker, add_menu: false, panel_open, rename: None, hover: Hover::default(), filter, organize: Organize::default(), closing: Closing::default() }, subs)
    }
}

impl Desktop {
    /// The Projects add button and its menu: add a project, or clone one.
    fn add_menu(&self, cx: &mut Context<Self>) -> Div {
        let open = self.sidebar.add_menu;
        let button = icon_button_sized("aside-add", "folder-plus", 22., TEXT_3).rounded(px(6.)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
            this.close_menus();
            this.sidebar.add_menu = !open;
            cx.notify();
        }));
        div().relative().child(button).when(open, |d| {
            let add = ui::menu_row("aside-add-project", "plus", "Add project…", None).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.add_local_project(window, cx)));
            let clone = ui::menu_row("aside-add-clone", "branch", "Clone from URL…", None).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.clone_project(window, cx)));
            let menu = ui::pop(div().id("aside-add-menu"))
                .w(px(220.))
                .p(px(6.))
                .flex()
                .flex_col()
                .occlude()
                .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                    this.sidebar.add_menu = false;
                    cx.notify();
                }))
                .child(add)
                .child(clone);
            d.child(ui::dropdown_right(26., ui::menu_in("aside-add-menu-in", menu)))
        })
    }

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
        let header = self.projects_header(cx);
        let body = div().id("aside-repos").flex_1().min_h_0().overflow_y_scroll().flex().flex_col().gap(px(1.)).children(self.sections(cx));
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

    /// `row`'s element under `p`; pinned, it names its project `sub`.
    fn place_row(&self, p: &str, row: &TreeRow, current: Option<&str>, setups: &HashMap<String, String>, sub: Option<String>, cx: &mut Context<Self>) -> AnyElement {
        match row.kind {
            Kind::Worktree => self.worktree_row(p, row, current, setups, sub, cx),
            Kind::Own | Kind::Local => self.local_row(p, row, current, sub, cx),
        }
    }

    /// A Local's row. The project's own, pinned, reads as the project with its Local's name `sub`.
    fn local_row(&self, p: &str, row: &TreeRow, current: Option<&str>, sub: Option<String>, cx: &mut Context<Self>) -> AnyElement {
        let (key, tip) = (row.key.as_str(), &row.tip);
        let selects = (row.kind == Kind::Local).then(|| row.key.clone());
        let menu = RowMenu::Local { project: p.to_string(), local: key.to_string() };
        let renames = self.agents.renames_offered(key);
        let pins = self.store.projects.iter().any(|k| k == p);
        let (selected, own) = (current == Some(key), selects.is_none());
        let state = rolled_state(&self.tree_cards(p, key));
        let mark = ui::indicator(id(format!("aside-spin:{key}")), state).or_else(|| self.pr_mark(key).filter(|_| own));
        let (name, sub) = match sub {
            Some(project) if own => (project, Some(self.local_name(key))),
            sub => (self.local_name(key), sub),
        };
        let label = match self.sidebar.rename.as_ref().filter(|r| r.tree == key) {
            Some(r) => Input::new(&r.input).appearance(false).p_0().text_size(px(13.)).into_any_element(),
            None => done_name(self.scrolling(key, name.clone(), tree_name(key, name.clone(), sweep_base(state, selected, cx.reduce_motion())), cx), state),
        };
        let label = with_sub(label, sub.clone(), self.sidebar.renaming(key));
        let mut buttons = Vec::new();
        if pins {
            buttons.push(self.pin_button(key, Pin::Tree { project: p.to_string(), tree: key.to_string() }, cx));
        }
        if pins || renames {
            buttons.push(self.row_menu_button(key, menu.clone(), cx));
        }
        let trail = (!buttons.is_empty()).then(|| ui::row_trail(None, buttons, self.row_menu.as_ref() == Some(&menu) || self.sidebar.hover.on(key)));
        let lit = self.sidebar.filter.lit(p, Some(key));
        let (target, path) = (p.to_string(), key.to_string());
        let glyph = if hosts::split(p).0.is_some() { "globe" } else { LOCAL_ICON };
        let row = ui::worktree_row(id(format!("aside-tree:{key}")), glyph, label, selected, mark)
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
            .when(sub.is_some(), |row| row.pl(px(6.)))
            .when(lit, |row| row.bg(FILL_3))
            .when(pins || renames, |row| row.on_mouse_down(MouseButton::Right, Self::open_row_menu(menu, cx)));
        let card = self.card_for(p, key, &name, tip, own, cx.reduce_motion());
        Self::with_card(row, key, card, cx).into_any_element()
    }

    /// `label`, reading `text`, scrolling through its cut-off end while `tree`'s card shows.
    fn scrolling(&self, tree: &str, text: String, label: AnyElement, cx: &mut Context<Self>) -> AnyElement {
        if cx.reduce_motion() || self.sidebar.hover.shows(tree).is_none() {
            return label;
        }
        let (this, row) = (cx.weak_entity(), tree.to_string());
        ui::marquee(id(format!("aside-marquee:{tree}")), text.into(), self.sidebar.hover.label(tree), label, move |width, overflow, cx| {
            this.update(cx, |this, cx| {
                if this.sidebar.hover.measured(&row, width, overflow) {
                    cx.notify();
                }
            })
            .ok();
        })
        .into_any_element()
    }

    /// A tree's pull request icon, if it has one.
    fn pr_mark(&self, tree: &str) -> Option<AnyElement> {
        self.prs.get(tree).map(|pr| {
            let (name, color) = pull_requests::glyph(pr);
            icon(name, 13., color).into_any_element()
        })
    }

    /// `tree`'s card, naming it `title` on `branch`, while it shows. `shows_pr` shows its PR.
    fn card_for(&self, p: &str, tree: &str, title: &str, branch: &str, shows_pr: bool, reduce_motion: bool) -> Option<Div> {
        self.sidebar.hover.shows(tree).map(|fades| {
            let when = self.tree_cards(p, tree).iter().map(|c| c.at).max().map(|at| ago_long(at, now_ms())).filter(|w| !w.is_empty());
            let slide = self.sidebar.hover.slide().filter(|_| !reduce_motion);
            tree_card::hung(title.to_string(), branch.to_string(), self.prs.get(tree).filter(|_| shows_pr), when, fades && !reduce_motion, slide)
        })
    }

    /// Shows `card` off `tree`'s row once the pointer rests there.
    fn with_card(row: Stateful<Div>, tree: &str, card: Option<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        let key = tree.to_string();
        row.relative()
            .on_hover(cx.listener(move |this, over: &bool, window, cx| this.hover_tree(&key, *over, window.mouse_position().y.into(), cx)))
            .on_any_mouse_down(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                if this.sidebar.hover.press() {
                    cx.notify();
                }
            }))
            .children(card)
    }

    fn hover_tree(&mut self, tree: &str, over: bool, y: f32, cx: &mut Context<Self>) {
        let now = Instant::now();
        // Rows reserve their trail by `Hover::on`, so redraw even when no card shows or hides.
        cx.notify();
        if !over {
            self.sidebar.hover.leave(tree, now);
            return;
        }
        if self.sidebar.hover.enter(tree, now, y) {
            return;
        }
        let tree = tree.to_string();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(tree_card::WAIT).await;
            this.update(cx, |d, cx| {
                if d.sidebar.hover.waited(&tree, Instant::now()) {
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// A project's row, then while it is open its checkout's and worktrees' rows, less those pinned.
    /// `only` holds the trees the filter lets through, unfolding it; `section` is where it sits, so where a project dropped on it goes.
    /// Returns its height once its fold settles, so a section around it can spring to it.
    fn project_block(&self, i: usize, p: &str, only: Option<&[String]>, section: Section, cx: &mut Context<Self>) -> (AnyElement, f32) {
        let current = self.place().filter(|_| self.screen == Screen::Sessions && self.project.as_deref() == Some(p));
        let kept = self.store.projects.iter().any(|k| k == p);
        let trees = self.trees_shown(p);
        let setups = self.creates.setups(&self.terminals.setups);
        let fold = ProjectRow::new(trees.as_deref(), only.is_none() && self.store.collapsed.contains(p), &setups);
        let selected = fold.selected(current.as_deref());
        let ProjectRow { git, open, setting_up } = fold;
        let state = if open { None } else { rolled_state(&self.cards(p)).filter(|_| !setting_up) };
        let chevron = git.then(|| ui::chevron(("aside-chevron", i), open));
        let menu = RowMenu::Project(p.to_string());
        let mut buttons = Vec::new();
        if kept {
            buttons.push(self.pin_button(p, Pin::Project(p.to_string()), cx));
        }
        if git {
            let target = p.to_string();
            buttons.push(
                icon_button_sized(id(format!("aside-plus:{p}")), "plus", 22., TEXT_3)
                    .rounded(px(6.))
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        cx.stop_propagation();
                        this.new_place(target.clone(), window, cx);
                    }))
                    .into_any_element(),
            );
        }
        buttons.push(self.row_menu_button(p, menu.clone(), cx));
        let setup = setting_up.then(|| ui::busy(id(format!("aside-setup:{p}")), "Setting up…").into_any_element());
        let trail = ui::row_trail(setup, buttons, self.row_menu.as_ref() == Some(&menu));
        let target = p.to_string();
        let name = self.repo_name(p);
        let host = hosts::split(p).0;
        let offline = host.is_some_and(|h| !self.fleet.links.online(h));
        let row = ui::repo_row(("aside-repo", i), chevron, &name, &self.project_mark(p, ui::mark_letter(&name)), selected, kept, state)
            .children(host.map(|h| host_mark(self.fleet.links.get(h))))
            .child(trail)
            .when(offline, |row| row.opacity(0.5))
            .when(self.sidebar.filter.lit(p, None), |row| row.bg(FILL_3))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                if git {
                    this.store.toggle(&target);
                    if this.store.collapsed.contains(&target) {
                        this.sidebar.closing.start(Fold::Project(target.clone()));
                    }
                    this.store.save();
                    cx.notify();
                } else {
                    this.select_tree(target.clone(), None, cx);
                }
            }))
            .on_mouse_down(MouseButton::Right, Self::open_row_menu(menu, cx))
            .when(kept, |row| row.on_drag(DragProject { path: p.to_string(), ix: i }, |_, _, _, cx| cx.new(|_| EmptyView)));
        let mut out = Vec::new();
        if self.sidebar.closing.drawn(&Fold::Project(p.to_string()), open) {
            let rows = self.tree_rows(p, trees.as_deref().unwrap_or_default());
            for row in rows.iter().filter(|r| only.is_none_or(|o| o.contains(&r.key)) && !self.tree_pinned(p, &r.key)) {
                out.push(self.place_row(p, row, current.as_deref(), &setups, None, cx));
            }
        }
        // The rows are fixed-height, so the open height is known without measuring: top pad, rows, gaps between them, bottom pad.
        let height = if out.is_empty() { 0. } else { 1. + fold::stacked(vec![ui::SIDEBAR_ROW_H; out.len()], 1.) + 6. };
        let rows = fold::springing(id(format!("aside-fold:{p}")), open, height, div().flex().flex_col().gap(px(1.)).when(!out.is_empty(), |d| d.pt(px(1.)).pb(px(6.))).children(out));
        let to = p.to_string();
        let manual = self.store.sections.sort == Sort::Manual;
        let block = div()
            .flex()
            .flex_col()
            .child(row)
            .child(rows)
            .when(kept, |d| {
                d.drag_over::<DragProject>(move |s, d, _, _| if d.ix == i || !manual { s } else { s.shadow(ui::drop_line(d.ix < i)) }).on_drop(cx.listener(move |this, d: &DragProject, _, cx| {
                    if manual {
                        this.store.move_project(&d.path, &to);
                    }
                    this.drop_into(&d.path, section);
                    cx.notify();
                }))
            });
        (block.into_any_element(), ui::SIDEBAR_ROW_H + if open { height } else { 0. })
    }

    /// A worktree's row under its project; pinned, it names its project `sub`.
    fn worktree_row(&self, p: &str, row: &TreeRow, current: Option<&str>, setups: &HashMap<String, String>, sub: Option<String>, cx: &mut Context<Self>) -> AnyElement {
        let (tree, title, tip) = (&row.key, &row.label, &row.tip);
        let menu = RowMenu::Tree { project: p.to_string(), tree: tree.clone() };
        let selected = current == Some(tree.as_str());
        let (failed, setting) = (self.creates.failed(tree), self::setting_up(setups, tree));
        let state = rolled_state(&self.tree_cards(p, tree)).filter(|_| !failed && !setting);
        let mark = if failed {
            ui::indicator(id(format!("aside-failed:{tree}")), Some(ui::State::Failed))
        } else if setting {
            Some(dot_spinner(id(format!("aside-setup:{tree}")), 11., TEXT_4).into_any_element())
        } else {
            ui::indicator(id(format!("aside-spin:{tree}")), state).or_else(|| self.pr_mark(tree))
        };
        let pin = Pin::Tree { project: p.to_string(), tree: tree.clone() };
        let mut buttons = Vec::new();
        if self.store.projects.iter().any(|k| k == p) {
            buttons.push(self.pin_button(tree, pin, cx));
        }
        buttons.push(self.row_menu_button(tree, menu.clone(), cx));
        let trail = ui::row_trail(None, buttons, self.row_menu.as_ref() == Some(&menu) || self.sidebar.hover.on(tree));
        let (target, path) = (p.to_string(), tree.clone());
        let label = match self.sidebar.rename.as_ref().filter(|r| &r.tree == tree) {
            Some(r) => Input::new(&r.input).appearance(false).p_0().text_size(px(13.)).into_any_element(),
            None => done_name(self.scrolling(tree, title.clone(), tree_name(tree, title.clone(), sweep_base(state, selected, cx.reduce_motion())), cx), state),
        };
        let pinned = sub.is_some();
        let row = ui::worktree_row(id(format!("aside-tree:{tree}")), WORKTREE_ICON, with_sub(label, sub, self.sidebar.renaming(tree)), selected, mark)
            .child(trail)
            .when(pinned, |row| row.pl(px(6.)))
            .when(self.sidebar.filter.lit(p, Some(tree)), |row| row.bg(FILL_3))
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
            .on_mouse_down(MouseButton::Right, Self::open_row_menu(menu, cx));
        let card = self.card_for(p, tree, title, tip, true, cx.reduce_motion());
        Self::with_card(row, tree, card, cx).into_any_element()
    }
}

/// `label`, then `sub` dimmed after it. An `editing` field has no width of its own, so it takes the room `sub` leaves.
fn with_sub(label: AnyElement, sub: Option<String>, editing: bool) -> AnyElement {
    match sub {
        Some(sub) => div().flex().items_center().gap(px(6.)).min_w_0().child(div().min_w_0().truncate().when(editing, |d| d.flex_1()).child(label)).child(div().flex_none().text_size(px(12.)).text_color(TEXT_3).child(sub)).into_any_element(),
        None => label,
    }
}

#[cfg(test)]
mod tests {
    use super::{ProjectRow, column_shown, in_tree, setting_up, sweep_base, tree_label};
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
    fn only_a_working_trees_label_sweeps_and_never_with_motion_reduced() {
        let got = [sweep_base(Some(ui::State::Working), false, false), sweep_base(Some(ui::State::Working), true, false), sweep_base(Some(ui::State::NeedsYou), false, false), sweep_base(None, false, false), sweep_base(Some(ui::State::Working), false, true)];
        assert_eq!(got, [Some(theme::TEXT_BODY), Some(theme::TEXT), None, None, None]);
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
