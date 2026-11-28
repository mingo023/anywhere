pub(crate) mod aside;
pub(crate) mod column;
mod filter;
pub(crate) mod fold;
pub(crate) mod header;
pub(crate) mod host;
pub(crate) mod panel;
pub(crate) mod project_picker;
pub(crate) mod rail;
pub(crate) mod rename;
mod reorder;
pub(crate) mod row_menu;
mod scratch;
pub(crate) mod sections;
pub(crate) mod sessions;
mod tree_card;
mod usage;

use crate::desktop::Desktop;
use crate::desktop::chrome::{Column, Layout, Overlay, RAIL, RowMenu, Screen, Side, id, state};
use crate::desktop::pages;
use crate::desktop::project::Placement;
use crate::git_ui::pull_requests;
use crate::glide::Glide;
use crate::inbox::Inbox;
use crate::modals::host_finder::Opener;
use crate::region;
use crate::renames::{self, Renames, Slot};
use crate::sidebar::column::ColumnView;
use crate::sidebar::fold::{Closing, Fold};
use crate::sidebar::header::{HeaderEvent, ProjectsHeader};
use crate::sidebar::panel::{Panel, PanelRail};
use crate::sidebar::project_picker::ProjectPicker;
use crate::sidebar::rail::Rail;
use crate::sidebar::rename::Rename;
use crate::sidebar::reorder::Reorder;
use crate::sidebar::sections::{Kind, Organize, TreeRow};
use crate::sidebar::sessions::SessionList;
use crate::sidebar::tree_card::Hover;
use crate::spaces::Space;
use crate::spaces::header::SpaceHeader;
use crate::spaces::switcher::Switcher;
use crate::spaces::theme_card::ThemeCard;
use crate::status::{self, Card};
use crate::util::{ago_long, basename, now_ms};
use agents::Agents;
use aside::{AsideFrame, CardFrame, RowMark, SidebarEvent, TreeFrame};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use store::sections::Sort;
use theme::*;

fn rolled_state(cards: &[Card]) -> Option<ui::State> {
    status::roll_up(cards.iter().map(|c| c.status)).map(|(s, _)| state(s, 0, 0))
}

/// The cards in `tree`: those opened in it as a Local, else those whose folder it holds, as `tree_of` places folders.
pub(crate) fn in_tree(cards: Vec<Card>, tree: Option<&str>, tree_of: impl Fn(&str) -> Option<String>) -> Vec<Card> {
    cards.into_iter().filter(|c| if c.local.is_empty() { tree_of(&c.cwd).as_deref() == tree } else { Some(c.local.as_str()) == tree }).collect()
}

/// The row a folder in `tree` sits under: `tree` while the project's pocketd lists it (`listed`, `None` until it has),
/// else the project's `main` one, as a folder in a worktree the project doesn't list belongs to its main one.
pub(crate) fn row_of(tree: Option<String>, listed: Option<&[agents::worktrees::Tree]>, main: impl FnOnce() -> Option<String>) -> Option<String> {
    match (&tree, listed) {
        (Some(t), Some(rows)) if !rows.iter().any(|r| &r.path == t) => main(),
        _ => tree,
    }
}

/// The column left of the page, listing Automations.
pub(crate) fn column_shown(layout: Layout, screen: Screen) -> bool {
    layout != Layout::Focus && screen == Screen::Automations
}

/// Where the lead column rests in `layout`, on a line from nothing through the rail to the sidebar.
fn lead_stop(layout: Layout) -> f32 {
    match layout {
        Layout::Focus => 0.,
        Layout::Compact => 1.,
        Layout::Sidebars => 2.,
    }
}

/// How wide the lead column is `at` a point on that line, with the sidebar `wide`.
fn lead_width(at: f32, wide: f32) -> f32 {
    RAIL * at.clamp(0., 1.) + (wide - RAIL) * aside_shown(at)
}

/// How far the sidebar has faded in over the rail `at` a point on that line.
fn aside_shown(at: f32) -> f32 {
    (at - 1.).clamp(0., 1.)
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
pub(crate) fn tree_place(w: &git::Worktree) -> String {
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
    (state == Some(ui::State::Working) && !reduce_motion).then_some(label_color(selected))
}

fn label_color(selected: bool) -> Token {
    if selected { TEXT } else { TEXT_BODY }
}

/// A tree row's label, sweeping from `sweeps` while its tree works.
fn tree_name(tree: &str, label: String, sweeps: Option<Token>) -> AnyElement {
    match sweeps {
        Some(base) => ui::sweep(id(format!("aside-sweep:{tree}")), label.into(), base).into_any_element(),
        None => label.into_any_element(),
    }
}

fn row_was(renames: &Renames, tree: &str, label: &str) -> Option<(String, Instant)> {
    let now = Instant::now();
    // An unnamed tree shows its branch, so a branch rename renames it too.
    [Slot::Name, Slot::Branch].into_iter().find_map(|slot| renames.was(tree, slot, label, now)).map(|(from, at)| (from.to_string(), at))
}

/// A tree row's label: decoding out of the one it replaced while a rename is fresh, else as `tree_name` draws it.
fn row_name(was: Option<&(String, Instant)>, tree: &str, label: String, state: Option<ui::State>, selected: bool, reduce_motion: bool) -> AnyElement {
    let was = was.map(|(from, at)| (from.as_str(), *at));
    renames::decoding(format!("aside-rename:{tree}:{label}"), was, &label, label_color(selected), reduce_motion)
        .unwrap_or_else(|| tree_name(tree, label, sweep_base(state, selected, reduce_motion)))
}

/// `label` bright and semibold while its tree is done, which lasts until it is viewed.
/// Styled outside the marquee, which measures the label in the style it inherits.
fn done_name(label: AnyElement, state: Option<ui::State>) -> AnyElement {
    match state {
        Some(ui::State::Done(..)) => div().text_color(TEXT).font_weight(FontWeight::SEMIBOLD).child(label).into_any_element(),
        _ => label,
    }
}

pub struct Sidebar {
    pub(crate) search: Entity<InputState>,
    pub(crate) sessions: Entity<SessionList>,
    pub(crate) space_header: Entity<SpaceHeader>,
    pub(crate) theme_card: Entity<ThemeCard>,
    /// The space buttons in the aside's foot and in the compact rail, which can be on screen together while the layout animates.
    pub(crate) aside_spaces: Entity<Switcher>,
    pub(crate) rail_spaces: Entity<Switcher>,
    /// Where a right-click opened the row menu; `None` drops it under its `···` button.
    pub(crate) menu_at: Option<Point<Pixels>>,
    pub(crate) picker: Entity<ProjectPicker>,
    pub(crate) inbox: Entity<Inbox>,
    pub(crate) rail: Entity<Rail>,
    pub(crate) panel_rail: Entity<PanelRail>,
    pub(crate) panel_view: Entity<Panel>,
    pub(crate) column: Entity<ColumnView>,
    /// The right-hand panel shows; collapsed at launch.
    panel_open: bool,
    /// The worktree row being renamed.
    pub(crate) rename: Option<Rename>,
    /// Which row's hover card shows.
    pub(crate) hover: Hover,
    pub(crate) header: Entity<ProjectsHeader>,
    pub(crate) organize: Organize,
    pub(crate) closing: Closing,
    /// Set only while the sidebar draws; see `Desktop::placed`.
    pub(crate) placement: RefCell<Option<Placement>>,
    frame: Option<AsideFrame>,
    /// The lead column, between its `lead_stop`s.
    pub(crate) lead: Glide,
    /// How far the right-hand panel is open.
    pub(crate) panel: Glide,
    reorder: Reorder,
}

impl Sidebar {
    pub fn new(panel_open: bool, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search sessions…"));
        let sessions = cx.new(|_| SessionList::new(search.clone()));
        let picker = cx.new(|cx| ProjectPicker::new(window, cx));
        let inbox = cx.new(|cx| Inbox::new(window, cx));
        let rail = cx.new(|_| Rail::new(picker.clone()));
        let panel_rail = cx.new(|_| PanelRail::default());
        let panel_view = cx.new(|_| Panel::default());
        let column = cx.new(|_| ColumnView::default());
        let header = cx.new(|cx| ProjectsHeader::new(window, cx));
        let theme_card = cx.new(|_| ThemeCard::new());
        let space_header = cx.new(|cx| SpaceHeader::new(theme_card.clone(), cx));
        let aside_spaces = cx.new(|_| Switcher::new("aside", false));
        let rail_spaces = cx.new(|_| Switcher::new("nav", true));
        Self {
            search,
            sessions,
            space_header,
            theme_card,
            aside_spaces,
            rail_spaces,
            menu_at: None,
            picker,
            inbox,
            rail,
            panel_rail,
            panel_view,
            column,
            panel_open,
            rename: None,
            hover: Hover::default(),
            header,
            organize: Organize::default(),
            closing: Closing::default(),
            placement: RefCell::default(),
            frame: None,
            lead: Glide::default(),
            panel: Glide::default(),
            reorder: Reorder::default(),
        }
    }

    pub(crate) fn panel_open(&self) -> bool {
        self.panel_open
    }

    pub(crate) fn set_panel_open(&mut self, open: bool, cx: &mut Context<Self>) {
        if self.panel_open != open {
            self.panel_open = open;
            cx.notify();
        }
    }

    /// Steps the lead column toward `layout` and the panel toward open or shut; true while either still moves.
    pub(crate) fn tick(&mut self, layout: Layout, now: Instant, reduce_motion: bool) -> bool {
        let lead = self.lead.tick(lead_stop(layout), now, reduce_motion);
        let panel = self.panel.tick(f32::from(u8::from(self.panel_open)), now, reduce_motion);
        lead || panel
    }

    /// How much of the columns Focus hides is showing.
    pub(crate) fn unfocused(&self) -> f32 {
        self.lead.at().clamp(0., 1.)
    }

    pub(crate) fn query(&self, cx: &App) -> String {
        self.header.read(cx).filter.query(cx)
    }

    pub(crate) fn lit(&self, cx: &App, project: &str, tree: Option<&str>) -> bool {
        self.header.read(cx).filter.lit(project, tree)
    }

    pub(crate) fn menu_open(&self, cx: &App) -> bool {
        self.picker.read(cx).open || self.inbox.read(cx).open || self.header.read(cx).menu_open() || self.space_header.read(cx).menu_open(cx)
    }

    pub(crate) fn close(&mut self, fold: Fold, cx: &mut Context<Self>) {
        self.closing.start(fold);
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(fold::SHUTTING + Duration::from_millis(50)).await;
            this.update(cx, |_, cx| cx.notify()).ok();
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn shut_menus(&mut self, cx: &mut Context<Self>) {
        cx.notify();
        self.menu_at = None;
        self.picker.update(cx, |p, cx| p.shut(cx));
        self.inbox.update(cx, |i, cx| i.shut(cx));
        self.header.update(cx, |h, cx| h.shut(cx));
        self.space_header.update(cx, |h, cx| h.shut(cx));
    }
}

impl Desktop {
    pub(crate) fn sidebar_subs(sidebar: &Entity<Sidebar>, window: &mut Window, cx: &mut Context<Self>) -> Vec<Subscription> {
        let (search, sessions, space_header, theme_card, aside, rail_spaces, picker, inbox, rail, panel_rail, panel_view, column, header) = {
            let s = sidebar.read(cx);
            (
                s.search.clone(),
                s.sessions.clone(),
                s.space_header.clone(),
                s.theme_card.clone(),
                s.aside_spaces.clone(),
                s.rail_spaces.clone(),
                s.picker.clone(),
                s.inbox.clone(),
                s.rail.clone(),
                s.panel_rail.clone(),
                s.panel_view.clone(),
                s.column.clone(),
                s.header.clone(),
            )
        };
        vec![
            cx.observe(sidebar, |_, _, cx| cx.notify()),
            cx.subscribe_in(sidebar, window, Self::on_sidebar_event),
            cx.subscribe(&search, |_, _, _: &InputEvent, cx| cx.notify()),
            cx.subscribe_in(&sessions, window, Self::on_sessions_event),
            cx.subscribe_in(&header, window, Self::on_header_event),
            cx.observe(&header, |_, _, cx| cx.notify()),
            cx.subscribe_in(&picker, window, Self::on_picker_event),
            cx.observe(&picker, |_, _, cx| cx.notify()),
            cx.subscribe_in(&inbox, window, Self::on_inbox_event),
            cx.observe(&inbox, |_, _, cx| cx.notify()),
            cx.subscribe_in(&space_header, window, Self::on_space_header_event),
            cx.subscribe_in(&theme_card, window, Self::on_theme_card_event),
            cx.subscribe_in(&aside, window, Self::on_switcher_event),
            cx.subscribe_in(&rail_spaces, window, Self::on_switcher_event),
            cx.subscribe_in(&rail, window, Self::on_rail_event),
            cx.subscribe_in(&panel_rail, window, Self::on_panel_rail_event),
            cx.subscribe_in(&panel_view, window, Self::on_panel_view_event),
            cx.subscribe_in(&column, window, Self::on_column_event),
        ]
    }

    fn on_header_event(&mut self, header: &Entity<ProjectsHeader>, ev: &HeaderEvent, window: &mut Window, cx: &mut Context<Self>) {
        match ev {
            HeaderEvent::Menu(menu) => {
                self.close_menus(cx);
                header.update(cx, |h, cx| h.show(*menu, cx));
            }
            HeaderEvent::OpenFilter => self.open_filter(window, cx),
            HeaderEvent::FilterKey(key) => self.filter_key(key, window, cx),
            HeaderEvent::Filtered => self.filtered(cx),
            HeaderEvent::FilterClosed => self.shut_filtered_folds(cx),
            HeaderEvent::NewGroup => self.new_group(None, window, cx),
            HeaderEvent::Sort(sort) => {
                self.store.sections.sort = *sort;
                self.store.save();
            }
            HeaderEvent::FoldAll(fold) => {
                let folds: Vec<Fold> = if *fold {
                    self.store.projects.iter().filter(|p| !self.store.collapsed.contains(*p)).cloned().map(Fold::Project).collect()
                } else {
                    vec![]
                };
                self.sidebar.update(cx, |s, cx| folds.into_iter().for_each(|f| s.close(f, cx)));
                self.store.collapsed = if *fold { self.store.projects.iter().cloned().collect() } else { Default::default() };
                self.store.save();
            }
            HeaderEvent::OpenFolder => self.open_folder(Opener::Sidebar, window, cx),
            HeaderEvent::CloneProject => self.clone_project(window, cx),
        }
        cx.notify();
    }

    fn on_sidebar_event(&mut self, _: &Entity<Sidebar>, ev: &SidebarEvent, window: &mut Window, cx: &mut Context<Self>) {
        match ev {
            SidebarEvent::Bell => self.toggle_inbox(cx),
            SidebarEvent::Collapse => self.toggle_sidebar(&crate::actions::ToggleSidebar, window, cx),
            SidebarEvent::Palette => self.open(Overlay::Palette, window, cx),
            SidebarEvent::Page(screen) => self.go_to_screen(*screen, window, cx),
            SidebarEvent::SelectTree { project, tree, progress } => {
                if let Some(tree) = tree.as_ref().filter(|_| *progress) {
                    self.creates.update(cx, |c, _| c.show_progress(tree));
                }
                self.select_tree(project.clone(), tree.clone(), cx);
            }
            SidebarEvent::NewPlace(project) => self.new_place(project.clone(), window, cx),
            SidebarEvent::ToggleProject(p) => {
                self.store.toggle(p);
                if self.store.collapsed.contains(p) {
                    self.sidebar.update(cx, |s, cx| s.close(Fold::Project(p.clone()), cx));
                }
                self.store.save();
                cx.notify();
            }
            SidebarEvent::OpenChanges => self.pick_side(Side::Changes, cx),
            SidebarEvent::Rename(tree) => self.start_rename(tree.clone(), window, cx),
            SidebarEvent::Renamed { tree, title } => self.save_rename(tree, title, cx),
            SidebarEvent::GroupRenamed { group, name } => {
                self.store.sections.rename_group(*group, name);
                self.store.save();
                cx.notify();
            }
            SidebarEvent::DropTree { project, from, to } => {
                let keys = self.tree_rows(project, &self.trees_shown(project, cx).unwrap_or_default(), cx).into_iter().map(|r| r.key).collect();
                self.store.sections.move_tree(project, keys, from, to);
                self.store.save();
                cx.notify();
            }
            SidebarEvent::DropProject { path, onto, section } => {
                if self.store.sections.sort == Sort::Manual {
                    self.store.move_project(path, onto);
                }
                self.drop_into(path, *section);
                cx.notify();
            }
            SidebarEvent::DropInto { path, section } => {
                self.drop_into(path, *section);
                cx.notify();
            }
            SidebarEvent::TogglePins => {
                self.store.sections.pins_folded = !self.store.sections.pins_folded;
                if self.store.sections.pins_folded {
                    self.sidebar.update(cx, |s, cx| s.close(Fold::Pinned, cx));
                }
                self.store.save();
                cx.notify();
            }
            SidebarEvent::ToggleGroup { id, clicks } => {
                // A double-click's second click undoes the fold its first made.
                self.store.sections.toggle_group(*id);
                if self.store.sections.group(*id).is_some_and(|g| g.folded) {
                    self.sidebar.update(cx, |s, cx| s.close(Fold::Group(*id), cx));
                }
                self.store.save();
                if *clicks > 1 {
                    return self.start_group_rename(*id, window, cx);
                }
                cx.notify();
            }
            SidebarEvent::ToggleScratch => {
                self.store.scratch_folded = !self.store.scratch_folded;
                if self.store.scratch_folded {
                    self.sidebar.update(cx, |s, cx| s.close(Fold::Scratch, cx));
                }
                self.store.save();
                cx.notify();
            }
            SidebarEvent::NewScratch => self.new_session_without_project(window, cx),
            SidebarEvent::DropScratch { from, to } => {
                self.store.move_project(from, to);
                self.store.save();
                cx.notify();
            }
            SidebarEvent::Menu(ev) => self.on_menu_event(ev.clone(), window, cx),
            SidebarEvent::Swipe(e) => self.swipe_spaces(e, window, cx),
            SidebarEvent::RestartToUpdate => self.restart_to_update(cx),
            SidebarEvent::DismissWhatsNew => self.dismiss_whats_new(cx),
            SidebarEvent::OpenFolder => self.open_folder(Opener::Sidebar, window, cx),
            SidebarEvent::CloneProject => self.clone_project(window, cx),
            SidebarEvent::AddHost => self.add_host_from_sidebar(window, cx),
            SidebarEvent::Settings => self.open_settings(&crate::actions::OpenSettings, window, cx),
            SidebarEvent::Drag(col, width) => self.drag_edge(*col, *width, cx),
            SidebarEvent::ResetWidth(col) => self.reset_width(*col, cx),
        }
    }

    pub(crate) fn open_filter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_menus(cx);
        self.sidebar.read(cx).header.clone().update(cx, |h, cx| h.open_filter(window, cx));
        cx.notify();
    }

    /// How wide the expanded sidebar is.
    pub(crate) fn aside_width(&self) -> f32 {
        self.width(Column::Projects, 272.)
    }

    /// How wide the lead column is now, as it moves between layouts.
    pub(crate) fn lead_width(&self, cx: &App) -> f32 {
        lead_width(self.sidebar.read(cx).lead.at(), self.aside_width())
    }

    /// The lead column: the sidebar or the rail, one fading into the other as it widens, and nothing in Focus.
    pub(crate) fn lead(&self, cx: &mut Context<Self>) -> Div {
        self.sync_inbox(cx);
        let (at, wide) = (self.sidebar.read(cx).lead.at(), self.aside_width());
        let shown = aside_shown(at);
        if at > 0. && shown < 1. {
            self.sync_rail(cx);
        }
        let rail = div().absolute().top_0().left_0().h_full().opacity(1. - shown).child(region::cached(&self.sidebar.read(cx).rail, region::column(RAIL)));
        if shown > 0. {
            self.sync_aside(cx);
        }
        let aside = div().flex_none().h_full().opacity(shown).child(region::cached(&self.sidebar, region::column(wide)));
        div()
            .relative()
            .flex()
            .flex_none()
            .h_full()
            .w(px(lead_width(at, wide)))
            .overflow_hidden()
            .when(at > 0. && shown < 1., |d| d.child(rail))
            .when(shown > 0., |d| d.child(aside))
    }

    /// `state` of a tree in `p`, as a row shows it: a host that can't be reached has nothing working, so its spinners stop.
    pub(super) fn calm(&self, p: &str, state: Option<ui::State>) -> Option<ui::State> {
        let unreachable = hosts::split(p).0.is_some_and(|h| !self.fleet.hosts.links.online(h));
        state.filter(|s| !(unreachable && *s == ui::State::Working))
    }

    pub(super) fn tree_cards(&self, project: &str, tree: &str, cx: &App) -> Vec<Card> {
        let listed = self.agents.projects.get(project).map(Vec::as_slice);
        in_tree(self.cards(project, cx), Some(tree), |cwd| row_of(self.tree_of(cwd, cx), listed, || self.tree_of(project, cx)))
    }

    pub(crate) fn sync_aside(&self, cx: &mut Context<Self>) {
        let (order, space) = (self.space_order(), self.space());
        self.space_header_set(&space, &order, cx);
        let aside_spaces = self.sidebar.read(cx).aside_spaces.clone();
        self.hand_spaces(&aside_spaces, &order, &space, cx);
        self.projects_header(cx);
        let frame = self.placed(cx, |cx| self.aside_frame(&space, cx));
        self.sidebar.update(cx, |s, cx| s.set_frame(frame, cx));
    }

    fn aside_frame(&self, space: &Space, cx: &mut Context<Self>) -> AsideFrame {
        let reduce_motion = cx.reduce_motion();
        let locked = self.locked_space().is_some();
        // A locked header hides the filter field, so its query mustn't keep filtering.
        let query = if locked { String::new() } else { self.sidebar.read(cx).query(cx) };
        AsideFrame {
            width: self.aside_width(),
            seam: self.overlay.is_none(),
            washed: self.wash().is_some(),
            reduce_motion,
            unseen: crate::inbox::count(&self.agents),
            inbox: Inbox::hung_from(&self.sidebar.read(cx).inbox, self.layout, Layout::Sidebars, cx),
            screen: self.screen,
            pages: pages::offered(&self.agents).map(|p| (p.name, p.icon, p.screen)).collect(),
            scratch: self.scratch_frame(&query, space, cx),
            locked,
            body: self.body_frame(&query, space, cx),
            slide: self.spaces.sliding(Instant::now()).filter(|_| !reduce_motion && !self.capturing),
            update: self.updates.update.clone(),
            star: self.star.read(cx).visible().then(|| self.star.clone()),
            menu: self.row_menu.as_ref().filter(|m| !matches!(m, RowMenu::Session(_))).and(self.menu_frame(cx)),
        }
    }

    /// A tree's pull request icon, if it has one.
    fn pr_mark(&self, tree: &str) -> Option<(&'static str, Token)> {
        self.prs.get(tree).map(pull_requests::glyph)
    }

    pub(super) fn tree_frame(
        &self,
        p: &str,
        row: &TreeRow,
        current: Option<&str>,
        setups: &HashMap<String, String>,
        sub: Option<String>,
        cx: &App,
    ) -> TreeFrame {
        let key = row.key.as_str();
        let selected = current == Some(key);
        let hovered = self.sidebar.read(cx).hover.shows(key).is_some();
        let when = || self.tree_cards(p, key, cx).iter().map(|c| c.at).max().map(|at| ago_long(at, now_ms())).filter(|w| !w.is_empty());
        let lit = self.sidebar.read(cx).lit(cx, p, Some(key));
        let base = |label: String, state, mark, pr, renames, menus, shows_pr: bool| TreeFrame {
            project: p.to_string(),
            key: key.to_string(),
            was: row_was(&self.renames, key, &label),
            card: hovered.then(|| CardFrame { pr: self.prs.get(key).filter(|_| shows_pr).cloned(), when: when() }),
            label,
            tip: row.tip.clone(),
            kind: row.kind,
            ix: row.ix,
            sub: None,
            selected,
            state,
            mark,
            pr,
            diff: None,
            lit,
            renames,
            menus,
        };
        match row.kind {
            Kind::Worktree => {
                let (failed, setting) = (self.creates.read(cx).failed(key), setting_up(setups, key));
                let state = self.calm(p, rolled_state(&self.tree_cards(p, key, cx)).filter(|_| !failed && !setting));
                let mark = if failed {
                    RowMark::Failed
                } else if setting {
                    RowMark::Setting
                } else {
                    RowMark::Status
                };
                let diff = self.repos.get(key).filter(|r| !r.files.is_empty()).map(|r| r.totals());
                TreeFrame { diff, sub, ..base(row.label.clone(), state, mark, self.pr_mark(key), self.agents.names_offered(), true, true) }
            }
            Kind::Own | Kind::Local => {
                let own = row.kind == Kind::Own;
                let renames = self.agents.renames_offered(key);
                let pins = self.store.projects.iter().any(|k| k == p);
                let state = self.calm(p, rolled_state(&self.tree_cards(p, key, cx)));
                let (label, sub) = match sub {
                    Some(project) if own => (project, Some(self.local_name(key))),
                    sub => (self.local_name(key), sub),
                };
                TreeFrame { sub, ..base(label, state, RowMark::Status, self.pr_mark(key).filter(|_| own), renames, pins || renames, own) }
            }
        }
    }
}

/// `label`, then `sub` dimmed after it. An `editing` field has no width of its own, so it takes the room `sub` leaves.
fn with_sub(label: AnyElement, sub: Option<String>, editing: bool) -> AnyElement {
    match sub {
        Some(sub) => div()
            .flex()
            .items_center()
            .gap(px(6.))
            .min_w_0()
            .child(div().min_w_0().truncate().when(editing, |d| d.flex_1()).child(label))
            .child(div().flex_none().text_size(px(12.)).text_color(TEXT_3).child(sub))
            .into_any_element(),
        None => label,
    }
}

#[cfg(test)]
mod tests {
    use super::{ProjectRow, aside_shown, column_shown, in_tree, lead_stop, lead_width, row_of, setting_up, sweep_base, tree_label};
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
    fn the_lead_column_widens_from_nothing_through_the_rail_to_the_sidebar() {
        assert_eq!([Layout::Focus, Layout::Compact, Layout::Sidebars].map(|l| lead_width(lead_stop(l), 272.)), [0., 56., 272.]);
        assert_eq!([lead_width(0.5, 272.), lead_width(1.5, 272.)], [28., 164.]);
    }

    #[test]
    fn the_sidebar_fades_in_only_past_the_rail() {
        assert_eq!([0., 0.5, 1., 1.25, 2.].map(aside_shown), [0., 0., 0., 0.25, 1.]);
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
    fn a_session_in_a_worktree_pocketd_leaves_off_the_sidebar_sits_under_the_main_row() {
        let listed = [agents::worktrees::Tree { path: "/p".into(), is_main: true, ..Default::default() }];
        let tree_of = |cwd: &str| if cwd.starts_with("/agents/fork") { Some("/agents/fork".to_string()) } else { tree_of(cwd) };
        let row = |cwd: &str| row_of(tree_of(cwd), Some(&listed), || Some("/p".into()));
        let cards = vec![card("a", "/p"), card("fork", "/agents/fork/src")];
        assert_eq!(ids(in_tree(cards, Some("/p"), row)), vec!["a", "fork"]);
        assert_eq!(row_of(Some("/wt".into()), None, || Some("/p".into())), Some("/wt".into()), "kept until pocketd lists the project");
    }

    #[test]
    fn no_tree_holds_the_cards_outside_every_tree() {
        let cards = vec![card("a", "/p"), card("d", "/lost")];
        assert_eq!(ids(in_tree(cards, None, tree_of)), vec!["d"]);
    }

    fn tree(path: &str, main: bool) -> git::Worktree {
        git::Worktree { path: path.into(), branch: String::new(), main, made: false, created: None }
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
        let named = git::Worktree { path: "/wt/calm-otter".into(), branch: "fix-login".into(), main: false, made: false, created: None };
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
        let got = [
            sweep_base(Some(ui::State::Working), false, false),
            sweep_base(Some(ui::State::Working), true, false),
            sweep_base(Some(ui::State::NeedsYou), false, false),
            sweep_base(None, false, false),
            sweep_base(Some(ui::State::Working), false, true),
        ];
        assert_eq!(got, [Some(theme::TEXT_BODY), Some(theme::TEXT), None, None, None]);
    }

    #[test]
    fn the_left_column_lists_automations_outside_focus() {
        let got = [
            column_shown(Layout::Sidebars, Screen::Sessions),
            column_shown(Layout::Compact, Screen::Automations),
            column_shown(Layout::Sidebars, Screen::Automations),
            column_shown(Layout::Focus, Screen::Automations),
        ];
        assert_eq!(got, [false, true, true, false]);
    }
}
