use daemon::Info;
use crate::termview::{self, Metrics};
use ui::{self, State, dot, icon_button_sized};
use theme::*;
use workspace::Tab;
use crate::status::{self, Kind};
use crate::{Card, Column, Desktop, Layout, Overlay, RowMenu, Screen, Side, Status};
use gpui_kit::component::input::Input;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::path::{Path, PathBuf};

pub fn basename(path: &str) -> String {
    path.trim_end_matches('/').rsplit('/').next().unwrap_or_default().to_string()
}

/// "/Users/me/code/app" reads as "~/code/app".
pub fn tilde(path: &str) -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    match path.strip_prefix(&home) {
        Some(rest) if !home.is_empty() && (rest.is_empty() || rest.starts_with('/')) => format!("~{rest}"),
        _ => path.to_string(),
    }
}

/// Two letters for the rail, from the repository's last dash-separated word: "app-android" is AN.
pub fn initials(name: &str) -> String {
    let word = name.rsplit('-').find(|w| !w.is_empty()).unwrap_or(name);
    word.chars().filter(|c| c.is_alphanumeric()).take(2).collect::<String>().to_uppercase()
}

pub fn list_dir(dir: &Path) -> Vec<(bool, PathBuf)> {
    let Ok(read) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut entries: Vec<(bool, PathBuf)> =
        read.flatten().filter(|e| e.file_name() != ".git").map(|e| (e.file_type().is_ok_and(|t| t.is_dir()), e.path())).collect();
    entries.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    entries
}

pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

pub fn ago(ms: i64, now: i64) -> String {
    if ms <= 0 {
        return String::new();
    }
    match (now - ms).max(0) / 60_000 {
        0 => "now".into(),
        m if m < 60 => format!("{m}m"),
        m if m < 24 * 60 => format!("{}h", m / 60),
        m if m < 48 * 60 => "yesterday".into(),
        m => format!("{}d", m / (24 * 60)),
    }
}

/// "12m ago", but "now" and "yesterday" as they are.
pub fn ago_long(ms: i64, now: i64) -> String {
    match ago(ms, now) {
        a if a.is_empty() || a == "now" || a == "yesterday" => a,
        a => format!("{a} ago"),
    }
}

/// "/bin/zsh -l" reads as "zsh": the login flag the desktop adds says nothing about the pane.
/// A login shell shows as its name only: the script it may run to start an agent is ours, not the user's.
pub fn command_line(info: &Info) -> String {
    if info.args.first().is_some_and(|a| a == "-l") {
        return basename(&info.cmd);
    }
    std::iter::once(basename(&info.cmd)).chain(info.args.iter().cloned()).collect::<Vec<_>>().join(" ")
}

pub fn id(s: String) -> ElementId {
    ElementId::Name(s.into())
}

pub fn drag_area(d: Div) -> Div {
    d.on_mouse_down(MouseButton::Left, |ev, window, _| {
        if ev.click_count == 1 {
            window.start_window_move();
        }
    })
}

pub fn state(status: Status, added: usize, removed: usize) -> State {
    match status {
        Status::NeedsYou => State::NeedsYou,
        Status::Failed => State::Failed,
        Status::Done => State::Done(added, removed),
        Status::Working => State::Working,
        Status::Idle => State::Idle(added, removed),
    }
}

fn row_mark(key: &str, setting_up: bool, cards: &[Card]) -> Option<AnyElement> {
    if setting_up {
        Some(ui::setting_up(id(format!("aside-setup:{key}"))).into_any_element())
    } else {
        let rolled = status::roll_up(cards.iter().map(|c| c.status)).map(|(s, _)| state(s, 0, 0));
        ui::indicator(id(format!("aside-spin:{key}")), rolled)
    }
}

#[derive(Clone)]
struct DragProject {
    path: String,
    ix: usize,
}

pub fn column() -> Div {
    ui::side(div().w(px(348.)).flex_none().h_full().flex().flex_col().overflow_hidden())
}

pub fn empty(text: impl Into<SharedString>) -> Div {
    div().p(px(16.)).text_size(px(13.5)).text_color(rgba(TEXT_3)).child(text.into())
}

impl Desktop {
    pub fn repo_name(&self, path: &str) -> String {
        self.store.repos.get(path).map(|r| r.name.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| basename(path))
    }

    pub fn repo_color(&self, path: &str) -> u32 {
        let fallback = || PALETTE[self.projects().iter().position(|p| p == path).unwrap_or(0) % PALETTE.len()];
        self.store.repos.get(path).map(|r| r.color).filter(|c| *c != 0).unwrap_or_else(fallback)
    }

    pub fn roll_up(&self, p: &str) -> Option<(Status, usize)> {
        status::roll_up(self.cards(p).iter().map(|c| c.status))
    }

    pub fn project_state(&self, p: &str) -> Option<State> {
        self.roll_up(p).map(|(s, _)| state(s, 0, 0))
    }

    pub fn open(&mut self, o: Overlay, window: &mut Window, cx: &mut Context<Self>) {
        self.close_menus();
        self.overlay = Some(o);
        match o {
            Overlay::Palette => self.open_palette(window, cx),
            Overlay::NewSession => self.reset_new_form(None, false, window, cx),
            Overlay::AddRepo => self.reset_repo_form(None, window, cx),
            Overlay::More | Overlay::Confirm => {}
        }
        cx.notify();
    }

    pub fn close_overlay(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.overlay = None;
        window.focus(&self.root, cx);
        cx.notify();
    }

    /// The user's width for `col`, the design's `fallback` until they drag its edge.
    pub fn width(&self, col: Column, fallback: f32) -> f32 {
        self.widths[col as usize].unwrap_or(fallback)
    }

    /// Lets the user drag `d`'s right edge to resize `col`.
    pub fn resizable(&self, d: Div, col: Column, cx: &mut Context<Self>) -> Div {
        // Occludes so the press starts a resize, not the drag area's window move.
        let handle = div()
            .id(("resize-column", col as usize))
            .absolute()
            .top_0()
            .bottom_0()
            .right_0()
            .w(px(5.))
            .occlude()
            .cursor_col_resize()
            .on_drag(col, |_, _, _, cx| {
                cx.stop_propagation();
                cx.new(|_| EmptyView)
            });
        d.relative().child(handle).on_drag_move(cx.listener(move |this, e: &DragMoveEvent<Column>, _, cx| {
            if *e.drag(cx) == col {
                this.drag_edge(col, f32::from(e.event.position.x - e.bounds.left()), cx);
            }
        }))
    }

    fn drag_edge(&mut self, col: Column, width: f32, cx: &mut Context<Self>) {
        let (min, max) = match col {
            Column::Projects => (200., 420.),
            Column::Sessions => (280., 600.),
        };
        self.widths[col as usize] = Some(width.clamp(min, max));
        cx.notify();
    }

    /// The expanded sidebar: projects with their worktrees.
    fn aside(&self, cx: &mut Context<Self>) -> Div {
        let top = drag_area(div())
            .h(px(46.))
            .flex()
            .flex_none()
            .items_center()
            .justify_end()
            .child(icon_button_sized("aside-bell", "bell", 28., TEXT_3).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open_inbox(window, cx))));
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
            .text_color(rgba(TEXT_3))
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
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.project_settings(&crate::ProjectSettings, window, cx))),
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
            .child(foot);
        self.resizable(aside, Column::Projects, cx)
    }

    fn tree_cards(&self, project: &str, tree: &str) -> Vec<Card> {
        self.cards(project).into_iter().filter(|c| self.tree_of(&c.cwd).as_deref() == Some(tree)).collect()
    }

    fn setting_up(&self, tree: &str) -> bool {
        self.setups.values().any(|t| t == tree)
    }

    fn open_row_menu(menu: RowMenu, cx: &mut Context<Self>) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static {
        cx.listener(move |this, _: &MouseDownEvent, _, cx| {
            this.close_menus();
            this.row_menu = Some(menu.clone());
            cx.notify();
        })
    }

    /// A project's row, then its worktrees' rows while it is open.
    fn project_block(&self, i: usize, p: &str, cx: &mut Context<Self>) -> AnyElement {
        let current = self.cwd().filter(|_| self.screen == Screen::Sessions && self.project.as_deref() == Some(p));
        let kept = self.store.projects.iter().any(|k| k == p);
        let main = self.tree_of(p).unwrap_or_else(|| p.to_string());
        let git = self.worktrees.get(p).is_some_and(|w| !w.is_empty());
        let open = git && !self.store.collapsed.contains(p);
        let cards = if open { self.tree_cards(p, &main) } else { self.cards(p) };
        let setting_up = !open && self.worktrees.get(p).into_iter().flatten().any(|w| self.setting_up(&w.path));
        let lead = if git {
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
        let selected = current.as_ref().is_some_and(|c| !open || *c == main);
        let row = ui::repo_row(("aside-repo", i), lead, &self.repo_name(p), selected, kept)
            .child(trail)
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.select_tree(target.clone(), None, cx)))
            .on_mouse_down(MouseButton::Right, Self::open_row_menu(menu, cx))
            .when(kept, |row| row.on_drag(DragProject { path: p.to_string(), ix: i }, |_, _, _, cx| cx.new(|_| EmptyView)));
        let mut out = vec![row.into_any_element()];
        if open {
            let trees: Vec<String> = self.worktrees[p].iter().filter(|w| !w.main).map(|w| w.path.clone()).collect();
            for tree in &trees {
                let menu = RowMenu::Tree { project: p.to_string(), tree: tree.clone() };
                let mark = row_mark(tree, self.setting_up(tree), &self.tree_cards(p, tree));
                let trail = ui::row_trail(mark, vec![self.row_menu_button(tree, menu.clone(), cx)], self.row_menu.as_ref() == Some(&menu));
                let (target, path) = (p.to_string(), tree.clone());
                out.push(
                    ui::worktree_row(id(format!("aside-tree:{tree}")), basename(tree), current.as_ref() == Some(tree))
                        .child(trail)
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.select_tree(target.clone(), Some(path.clone()), cx)))
                        .on_mouse_down(MouseButton::Right, Self::open_row_menu(menu, cx))
                        .into_any_element(),
                );
            }
            if trees.is_empty() {
                out.push(self.new_worktree_row(p, cx));
            }
        }
        let to = p.to_string();
        div()
            .flex()
            .flex_col()
            .gap(px(1.))
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

    fn row_menu_button(&self, key: &str, menu: RowMenu, cx: &mut Context<Self>) -> AnyElement {
        let open = self.row_menu.as_ref() == Some(&menu);
        let toggle = menu.clone();
        let button = icon_button_sized(id(format!("aside-more:{key}")), "more", 22., TEXT_3).rounded(px(6.)).capture_any_mouse_down(cx.listener(
            move |this, _: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                this.row_menu = (this.row_menu.as_ref() != Some(&toggle)).then(|| toggle.clone());
                this.tab_menu = false;
                cx.notify();
            },
        ));
        div()
            .relative()
            .child(button)
            .when(open, |d| {
                d.child(ui::dropdown(
                    26.,
                    ui::pop(div().id("aside-menu")).w(px(210.)).p(px(6.)).rounded(px(14.)).flex().flex_col()
                        .occlude()
                        .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                            this.row_menu = None;
                            cx.notify();
                        }))
                        .children(self.row_menu_items(&menu, cx)),
                ))
            })
            .into_any_element()
    }

    fn row_menu_items(&self, menu: &RowMenu, cx: &mut Context<Self>) -> Vec<AnyElement> {
        match menu.clone() {
            RowMenu::Project(p) => {
                let kept = self.store.projects.contains(&p);
                let target = p.clone();
                let first = if kept {
                    ui::menu_row("aside-menu-settings", "settings", "Settings…", None).on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.select_project(target.clone(), cx);
                        this.project_settings(&crate::ProjectSettings, window, cx);
                    }))
                } else {
                    ui::menu_row("aside-menu-keep", "check", "Keep in Pocket", None).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.row_menu = None;
                        this.keep_project(&target, cx);
                    }))
                };
                vec![
                    first.into_any_element(),
                    ui::menu_divider().into_any_element(),
                    ui::danger_row("aside-menu-remove", "x", if kept { "Remove from Pocket" } else { "Remove" })
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.row_menu = None;
                            this.ask_remove_project(p.clone(), cx);
                        }))
                        .into_any_element(),
                ]
            }
            RowMenu::Tree { project, tree } => vec![
                ui::danger_row("aside-menu-delete", "trash", "Delete worktree…")
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.row_menu = None;
                        this.ask_delete_worktree(project.clone(), tree.clone(), cx);
                    }))
                    .into_any_element(),
            ],
        }
    }

    fn new_worktree_row(&self, p: &str, cx: &mut Context<Self>) -> AnyElement {
        let target = p.to_string();
        div()
            .id(id(format!("aside-new-worktree:{p}")))
            .h(px(32.))
            .pl(px(25.))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(7.))
            .rounded(px(9.))
            .cursor_pointer()
            .text_size(px(13.))
            .text_color(rgba(TEXT_4))
            .hover(|s| s.bg(rgba(FILL_2)))
            .child(div().w(px(22.)).flex().flex_none().justify_center().child(icon("plus", 12., TEXT_4)))
            .child("New worktree")
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.new_worktree_in(target.clone(), window, cx)))
            .into_any_element()
    }

    /// Context left in each provider's newest open session.
    fn usage(&self) -> Vec<(&'static str, u64)> {
        let latest = |p: &str| {
            let a = self.agents.list.iter().filter(|a| a.provider == p && a.status != "closed").max_by_key(|a| a.updated_at)?;
            self.agents.context_left(&a.id)
        };
        ["claude", "codex"].into_iter().filter_map(|p| latest(p).map(|left| (p, left))).collect()
    }

    fn usage_card(&self) -> Option<Div> {
        let parts: Vec<Div> =
            self.usage().into_iter().map(|(p, left)| div().flex().items_center().gap(px(6.)).child(dot(7., provider_color(p))).child(format!("{left}%"))).collect();
        (!parts.is_empty()).then(|| {
            div()
                .mt(px(8.))
                .mx(px(2.))
                .py(px(8.))
                .px(px(10.))
                .flex()
                .items_center()
                .gap(px(12.))
                .rounded(px(12.))
                .bg(rgba(0xffffff8c))
                .shadow(vec![ui::ring(FILL_3, 0.5)])
                .text_size(px(12.))
                .text_color(rgba(TEXT_2))
                .children(parts)
                .child(div().ml_auto().text_color(rgba(TEXT_4)).child("context left"))
        })
    }

    /// The compact layout's rail: the project's non-idle sessions and the selected one, other non-idle repositories, and new session.
    fn nav(&self, cx: &mut Context<Self>) -> Div {
        let rule = || div().w(px(28.)).h(px(0.5)).my(px(4.)).flex_none().bg(rgba(SEPARATOR_STRONG));
        let mark = |name: &str, selected: bool, state: Option<State>| {
            ui::repo_mark(name, selected, state).size(px(24.)).text_size(px(12.))
        };
        let toggle = div()
            .id("nav-panel")
            .w(px(36.))
            .h(px(32.))
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .rounded(px(8.))
            .cursor_pointer()
            .when(self.panel, |d| d.bg(rgba(FILL_3)))
            .hover(|s| s.bg(rgba(FILL_3)))
            .child(icon("sidebar", 18., TEXT_2))
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.toggle_rail(&crate::ToggleRail, window, cx)));
        let project = self.project.clone().unwrap_or_default();
        let badge = |d: Div, color: u32| d.absolute().right(px(3.)).size(px(8.)).rounded(px(4.)).bg(rgba(color)).shadow(vec![ui::ring(SURFACE_SUNKEN, 2.)]);
        let mut live: Vec<Card> = self.cards(&project).into_iter().filter(|c| c.status != Status::Idle || self.session.as_ref() == Some(&c.id)).collect();
        live.sort_by_key(|c| c.status);
        let sessions = live.into_iter().enumerate().map(|(i, c)| {
            let selected = self.session.as_ref() == Some(&c.id);
            let id = c.id.clone();
            div()
                .id(("nav-session", i))
                .relative()
                .size(px(36.))
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .rounded(px(9.))
                .cursor_pointer()
                .when(selected, |d| d.bg(rgba(WHITE)).shadow(ui::row_shadow()))
                .when(!selected, |d| d.hover(|s| s.bg(rgba(FILL_2))))
                .child(icon("terminal", 17., provider_color(&c.provider)))
                .children(ui::alert_color(state(c.status, 0, 0)).map(|color| badge(div().top(px(3.)), color)))
                .when(c.status == Status::Working, |d| d.child(badge(div().bottom(px(3.)), RUNNING)))
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.focus_agent(&id, window, cx)))
        });
        let repos = self.projects().into_iter().filter(|p| *p != project).filter_map(|p| self.project_state(&p).map(|st| (p, st))).enumerate().map(|(i, (p, st))| {
            div()
                .id(("nav-repo", i))
                .w(px(36.))
                .h(px(34.))
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .rounded(px(9.))
                .cursor_pointer()
                .hover(|s| s.bg(rgba(FILL_2)))
                .child(mark(&self.repo_name(&p), false, Some(st)))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.panel = true;
                    this.select_project(p.clone(), cx);
                }))
        });
        let compose = div()
            .id("nav-compose")
            .size(px(36.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(9.))
            .cursor_pointer()
            .child(icon("compose", 17., WHITE))
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open(Overlay::NewSession, window, cx)));
        let bars = self.usage().into_iter().map(|(p, left)| {
            div().w(px(24.)).h(px(3.)).flex().rounded(px(2.)).bg(rgba(SEPARATOR_STRONG)).child(div().w(relative(left as f32 / 100.)).rounded(px(2.)).bg(rgba(provider_color(p))))
        });
        let me = if self.initials.is_empty() { "ME".to_string() } else { self.initials.clone() };
        drag_area(ui::side(div()))
            .w(px(56.))
            .flex_none()
            .h_full()
            .pt(px(37.))
            .pb(px(12.))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(6.))
            .child(toggle)
            .child(rule())
            .child(div().mb(px(2.)).child(mark(&self.repo_name(&project), true, None)))
            .children(sessions)
            .child(rule())
            .children(repos)
            .child(
                div()
                    .mt_auto()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(8.))
                    .child(ui::primary(compose))
                    .child(div().py(px(4.)).flex().flex_col().items_center().gap(px(3.)).children(bars))
                    .child(ui::avatar(&me, 30.).text_size(px(10.5))),
            )
    }

    /// The compact layout's sidebars, floated over a dimmed page.
    fn panel_view(&mut self, cx: &mut Context<Self>) -> Div {
        let dim = div()
            .id("panel-dim")
            .absolute()
            .inset_0()
            .bg(rgba(0x1111131a))
            .occlude()
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.panel = false;
                cx.notify();
            }));
        let column = self.column_view(cx);
        // GPUI has no backdrop blur, so the translucent sidebars sit on the dimmed window colour instead of over the page's text.
        let sidebars = div().h_full().flex().bg(rgba(WINDOW)).shadow(vec![BoxShadow { offset: point(px(16.), px(0.)), ..ui::shadow(0x11111324, 0., 48.) }]).child(div().h_full().flex().bg(rgba(0x1111131a)).child(self.aside(cx)).child(column));
        div().absolute().top_0().bottom_0().left(px(56.)).right_0().flex().child(dim).child(sidebars)
    }

    fn column_view(&mut self, cx: &mut Context<Self>) -> Div {
        let body = match (self.screen, self.side) {
            (Screen::Inbox, _) => {
                let list = self.inbox_list(cx).w(px(self.width(Column::Sessions, 348.)));
                return self.resizable(list, Column::Sessions, cx);
            }
            (_, Side::Sessions) => self.session_list(cx).into_any_element(),
            (_, Side::Explorer) => self.explorer(cx).into_any_element(),
            (_, Side::Changes) => self.changes_list(cx).into_any_element(),
        };
        let tabs = [(Side::Sessions, "Sessions"), (Side::Explorer, "Explorer"), (Side::Changes, "Changes")].into_iter().enumerate().map(|(i, (side, label))| {
            let selected = self.side == side;
            div()
                .id(("column-tab", i))
                .flex_1()
                .h(px(30.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(8.))
                .cursor_pointer()
                .text_size(px(13.))
                .when(selected, |d| d.bg(rgba(FILL_4)).text_color(rgba(TEXT)).font_weight(FontWeight::SEMIBOLD))
                .when(!selected, |d| d.text_color(rgba(TEXT_2)).font_weight(FontWeight::MEDIUM).hover(|s| s.bg(rgba(FILL_2))))
                .child(label)
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.side = side;
                    if side == Side::Changes {
                        this.load_diff(cx);
                    }
                    cx.notify();
                }))
        });
        let tabs = div().p(px(8.)).flex().flex_none().gap(px(4.)).border_b(px(0.5)).border_color(rgba(SEPARATOR)).children(tabs);
        let column = column()
            .w(px(self.width(Column::Sessions, 334.)))
            .child(drag_area(self.column_header(cx)).h(px(42.)).flex_none().border_b(px(0.5)).border_color(rgba(SEPARATOR)))
            .child(tabs)
            .child(body);
        self.resizable(column, Column::Sessions, cx)
    }

    fn column_header(&self, cx: &mut Context<Self>) -> Div {
        let search = icon_button_sized("column-search", "search", 28., TEXT_2)
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open(Overlay::Palette, window, cx)));
        let add = icon_button_sized("column-add", "plus", 28., TEXT_2)
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open(Overlay::NewSession, window, cx)));
        div()
            .pl(px(18.))
            .pr(px(12.))
            .flex()
            .items_center()
            .gap(px(4.))
            .child(div().flex_1().text_size(px(16.)).font_weight(FontWeight::BOLD).child("Workspace"))
            .child(search)
            .child(add)
    }

    fn session_list(&mut self, cx: &mut Context<Self>) -> Div {
        let search = div()
            .h(px(44.))
            .flex_none()
            .pl(px(18.))
            .pr(px(14.))
            .flex()
            .items_center()
            .gap(px(9.))
            .border_b(px(0.5))
            .border_color(rgba(SEPARATOR))
            .child(icon("search", 14., TEXT_3))
            .child(div().flex_1().text_size(px(13.)).child(Input::new(&self.session_search).appearance(false).p_0().text_size(px(13.))));
        let list = div().id("cards").flex_1().min_h_0().overflow_y_scroll();
        let wrap = div().flex_1().min_h_0().flex().flex_col().child(search);
        let Some(project) = self.project.clone() else {
            return wrap.child(list.child(empty("Add a project with + to start.")));
        };
        let tree = self.cwd();
        let query = self.session_search.read(cx).value().to_lowercase();
        let mut cards: Vec<Card> = self
            .cards(&project)
            .into_iter()
            .filter(|c| self.tree_of(&c.cwd) == tree && c.title.to_lowercase().contains(&query))
            .collect();
        cards.sort_by_key(|c| c.status != Status::NeedsYou);
        let cards: Vec<_> = cards.into_iter().enumerate().map(|(i, c)| self.card(i, c, cx)).collect();
        wrap.child(list.child(div().p(px(8.)).flex().flex_col().gap(px(2.)).children(cards)))
    }

    fn card(&self, i: usize, c: Card, cx: &mut Context<Self>) -> Stateful<Div> {
        let selected = self.session.as_ref() == Some(&c.id);
        let (added, removed) = self.repos.get(&c.cwd).map(|r| r.totals()).unwrap_or_default();
        let id = c.id.clone();
        let pill = match c.kind {
            Kind::NotAttached => State::NotAttached,
            _ => state(c.status, added, removed),
        };
        let lead = ui::provider_label(&c.provider, c.kind == Kind::Ended);
        let branch = self.repos.get(&c.cwd).map(|r| r.branch.clone());
        ui::session_row(("card", i), selected, lead, ago(c.at, now_ms()), c.title, branch, Some(pill))
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.focus_agent(&id, window, cx)))
    }

    fn main_view(&mut self, cx: &mut Context<Self>) -> Div {
        let body = match (self.screen, self.side) {
            (Screen::Inbox, _) => self.inbox_detail(cx),
            (_, Side::Changes) if self.diff_file.is_some() => self.diff_view(cx),
            (_, Side::Explorer) if self.file.is_some() => self.file_view(cx),
            _ => match self.cwd().filter(|t| !self.workspace(t).tabs.is_empty()) {
                Some(tree) => self.session_page(&tree, cx),
                None => self.blank_page(cx),
            },
        };
        ui::page(div()).flex_1().min_w_0().h_full().flex().flex_col().overflow_hidden().child(body)
    }

    fn blank_page(&self, cx: &mut Context<Self>) -> Div {
        let text = if self.project.is_none() { "Add a project to begin." } else { "Pick a session, or start a new one." };
        div().flex_1().flex().flex_col().child(self.page_bar(vec!["Sessions".into()], Vec::new(), div(), cx)).child(
            div()
                .flex_1()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(14.))
                .text_size(px(14.))
                .text_color(rgba(TEXT_3))
                .child(text)
                .when(self.project.is_some(), |d| {
                    d.child(
                        ui::button("blank-new", ui::Variant::Primary, Some("sparkle"), "New session")
                            .child(ui::button_kbd("⌘N"))
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open(Overlay::NewSession, window, cx))),
                    )
                }),
        )
    }

    pub fn pane_label(&self, id: &str) -> String {
        match (self.summary(id), self.sessions.get(id)) {
            (Some(a), _) => a.provider.clone(),
            (None, Some(s)) => s.busy().map_or_else(|| command_line(&s.info), str::to_string),
            (None, None) => "session".into(),
        }
    }

    /// A top bar's left padding and the sidebar toggle it starts with.
    fn bar_start(&self, cx: &mut Context<Self>) -> (f32, Option<Stateful<Div>>) {
        let toggle = |name: &str, cx: &mut Context<Self>| {
            icon_button_sized("focus-toggle", name, 28., TEXT_2).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.toggle_focus(&crate::ToggleFocus, window, cx)))
        };
        match self.layout {
            // Leaves room for the window's traffic lights once the sidebars are hidden.
            Layout::Focus => (82., Some(toggle("sidebar-expand", cx))),
            Layout::Compact => (14., None),
            Layout::Sidebars => (24., None),
        }
    }

    /// The page's top bar: sidebar toggle, breadcrumb and meta on the left, `right` on the far side.
    pub fn page_bar(&self, crumbs: Vec<String>, meta: Vec<AnyElement>, right: impl IntoElement, cx: &mut Context<Self>) -> Div {
        let (pad, toggle) = self.bar_start(cx);
        drag_area(ui::page_bar())
            .pl(px(pad))
            .children(toggle)
            .child(ui::breadcrumb(crumbs))
            .child(ui::meta_row(meta))
            .child(div().ml_auto().flex().flex_none().items_center().gap(px(8.)).child(right))
    }

    fn session_page(&mut self, tree: &str, cx: &mut Context<Self>) -> Div {
        let (added, removed) = self.repo().map(|r| r.totals()).unwrap_or_default();
        let diff = (added + removed > 0).then(|| {
            div()
                .id("bar-diff")
                .cursor_pointer()
                .child(ui::meta_diff(added, removed, 12.))
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.open_changes(None, cx)))
        });
        let error = self.error.clone().map(|e| div().min_w_0().truncate().text_size(px(12.5)).text_color(rgba(FAILED)).child(e));
        let status = div().ml_auto().mr(px(4.)).pl(px(8.)).min_w_0().flex().items_center().gap(px(12.)).children(error).children(diff);
        let right = div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(8.))
            .child(ui::icon_group([
                ui::group_button("split-right", "split-right").on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.new_shell(Some(false), cx))),
                ui::group_button("split-down", "split-down").on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.new_shell(Some(true), cx))),
            ]))
            .child(ui::icon_group([
                ui::group_button("session-more", "more").on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open(Overlay::More, window, cx))),
            ]));
        let (pad, toggle) = self.bar_start(cx);
        let bar = drag_area(div())
            .h(px(42.))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(6.))
            .pl(px(pad))
            .pr(px(10.))
            .children(toggle)
            .child(self.term_tabs(tree, cx))
            .child(status)
            .child(right);
        let body = match self.workspace(tree).active() {
            Some(Tab::Term(rows)) => {
                let rows = rows.clone();
                self.panes(rows, cx)
            }
            Some(Tab::Changes) => self.diff_box(cx),
            None => div().flex_1(),
        };
        div().flex_1().min_h_0().flex().flex_col().bg(rgba(SURFACE_SUNKEN)).child(bar).child(body)
    }

    fn tab_lead(&self, panes: Option<Vec<String>>) -> Div {
        let row = div().flex().items_center().gap(px(7.));
        let label = |text: String| div().max_w(px(150.)).truncate().child(text);
        let Some(p) = panes else {
            return row.child(icon("branch", 13., TEXT_3)).child("Changes");
        };
        let count = |text: String| if p.len() > 1 { format!("{text} · {} panes", p.len()) } else { text };
        if let Some(a) = self.summary(&p[0]) {
            let mark = match Status::of(a) {
                Some(Status::NeedsYou) => Some(dot(6., WAITING).into_any_element()),
                Some(Status::Failed) => Some(icon("x", 12., FAILED).into_any_element()),
                Some(Status::Done) => Some(dot(6., ACCENT).into_any_element()),
                Some(Status::Working) => Some(dot(6., RUNNING).into_any_element()),
                _ => None,
            };
            return row.child(dot(7., provider_color(&a.provider))).child(label(count(provider_name(&a.provider).into()))).children(mark);
        }
        let s = self.sessions.get(&p[0]);
        let busy = s.and_then(|s| s.busy());
        let mark = match s {
            Some(s) if s.failed() => icon("x", 12., FAILED).into_any_element(),
            _ if busy.is_some() => dot(6., RUNNING).into_any_element(),
            _ => dot(6., TEXT_5).into_any_element(),
        };
        let text = busy.map_or_else(|| self.pane_label(&p[0]), str::to_string);
        row.child(icon("prompt", 13., TEXT_3)).child(label(count(text))).child(mark)
    }

    fn term_tabs(&mut self, tree: &str, cx: &mut Context<Self>) -> Div {
        let w = self.workspace(tree);
        let active = w.active;
        let tabs: Vec<Option<Vec<String>>> =
            w.tabs.iter().map(|t| if let Tab::Term(r) = t { Some(r.iter().flatten().cloned().collect()) } else { None }).collect();
        let items: Vec<_> = tabs
            .into_iter()
            .enumerate()
            .map(|(i, panes)| {
                let selected = i == active;
                let close = div()
                    .id(("close-tab", i))
                    .size(px(20.))
                    .mr(px(4.))
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .rounded(px(5.))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgba(FILL_3)))
                    .child(icon("x", 11., TEXT_4))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        this.close_tab(i, cx);
                    }));
                let tab = div()
                    .id(("tab", i))
                    .h(px(28.))
                    .pl(px(10.))
                    .pr(px(6.))
                    .flex()
                    .items_center()
                    .cursor_pointer()
                    .child(self.tab_lead(panes))
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.select_tab(i, window, cx)));
                div()
                    .h(px(28.))
                    .flex()
                    .flex_none()
                    .items_center()
                    .rounded(px(7.))
                    .text_size(px(12.5))
                    .whitespace_nowrap()
                    .when(selected, |d| d.bg(rgba(WHITE)).shadow(ui::row_shadow()).font_weight(FontWeight::SEMIBOLD).text_color(rgba(TEXT)))
                    .when(!selected, |d| d.font_weight(FontWeight::MEDIUM).text_color(rgba(TEXT_2)).hover(|s| s.bg(rgba(FILL_2))))
                    .child(tab)
                    .child(close)
            })
            .collect();
        let plus = div()
            .id("new-tab")
            .size(px(28.))
            .ml(px(2.))
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .rounded(px(7.))
            .cursor_pointer()
            .hover(|s| s.bg(rgba(FILL_3)))
            .child(icon("plus", 15., TEXT_2))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.new_shell(None, cx)));
        let chevron = div()
            .id("tab-menu-toggle")
            .w(px(20.))
            .h(px(28.))
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .rounded(px(6.))
            .cursor_pointer()
            .when(self.tab_menu, |d| d.bg(rgba(FILL_3)))
            .hover(|s| s.bg(rgba(FILL_3)))
            .child(icon("chevron-down", 12., TEXT_3))
            // Runs before the open menu's click-outside handler, which would otherwise close it only for this click to reopen it.
            .capture_any_mouse_down(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                this.tab_menu = !this.tab_menu;
                this.row_menu = None;
                cx.notify();
            }));
        let menu = self.tab_menu.then(|| ui::dropdown(29., self.tab_menu_view(cx)));
        div()
            .flex()
            .flex_initial()
            .min_w_0()
            .h(px(40.))
            .items_center()
            .gap(px(2.))
            .child(div().flex().min_w_0().items_center().gap(px(2.)).overflow_hidden().children(items))
            .child(div().relative().flex().flex_none().items_center().gap(px(2.)).child(plus).child(chevron).children(menu))
    }

    /// The last model seen for `provider`, as a short label.
    pub fn model_hint(&self, provider: &str) -> Option<String> {
        let latest = self.agents.list.iter().filter(|a| a.provider == provider && a.model.is_some()).max_by_key(|a| a.updated_at)?;
        Some(agents::model_label(latest))
    }

    fn tab_menu_view(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let branch = self.repo().map(|r| r.branch.clone()).unwrap_or_default();
        let item = |id: &'static str, lead: AnyElement, label: String, hint: Option<String>, keys: Option<&str>| {
            div()
                .id(id)
                .h(px(32.))
                .px(px(8.))
                .flex()
                .flex_none()
                .items_center()
                .gap(px(9.))
                .rounded(px(6.))
                .cursor_pointer()
                .text_size(px(13.))
                .hover(|s| s.bg(rgba(FILL_2)))
                .child(div().w(px(16.)).flex().flex_none().justify_center().child(lead))
                .child(div().flex_1().flex().whitespace_nowrap().child(label).children(hint.map(|h| div().ml(px(7.)).text_color(rgba(TEXT_3)).child(h))))
                .children(keys.map(|k| div().text_size(px(11.5)).text_color(rgba(TEXT_4)).child(k.to_string())))
        };
        let agent = |id: &'static str, provider: &'static str, cx: &mut Context<Self>| {
            item(id, dot(8., provider_color(provider)).into_any_element(), provider_name(provider).into(), self.model_hint(provider), None)
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.new_agent_tab(provider, cx)))
        };
        ui::pop(div().id("tab-menu"))
            .w(px(264.))
            .p(px(6.))
            .rounded(px(10.))
            .flex()
            .flex_col()
            .gap(px(1.))
            .child(
                div()
                    .pt(px(4.))
                    .px(px(8.))
                    .pb(px(6.))
                    .flex()
                    .text_size(px(11.5))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgba(TEXT_3))
                    .child("New tab in ")
                    .child(div().font_family(MONO).font_weight(FontWeight::MEDIUM).child(branch)),
            )
            .child(
                item("tab-menu-shell", icon("prompt", 14., TEXT_2).into_any_element(), "New shell".into(), None, Some("⌘T"))
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.new_tab(&crate::NewTab, window, cx))),
            )
            .child(div().h(px(0.5)).my(px(4.)).mx(px(6.)).bg(rgba(SEPARATOR)))
            .child(agent("tab-menu-claude", "claude", cx))
            .child(agent("tab-menu-codex", "codex", cx))
            .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                this.tab_menu = false;
                cx.notify();
            }))
    }

    fn panes(&mut self, rows: Vec<Vec<String>>, cx: &mut Context<Self>) -> Div {
        let split = rows.len() > 1 || rows[0].len() > 1;
        let mut n = 0;
        let mut out = Vec::new();
        for (r, row) in rows.into_iter().enumerate() {
            let m = if r == 0 { &termview::MAIN } else { &termview::SMALL };
            let panes: Vec<Div> = row
                .into_iter()
                .map(|id| {
                    n += 1;
                    self.pane(&id, split.then_some(n), m, cx)
                })
                .collect();
            out.push(div().flex().gap(px(0.5)).min_h_0().when(r == 0, |d| d.flex_1()).when(r > 0, |d| d.h(px(250.)).flex_none()).children(panes));
        }
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .gap(px(0.5))
            .bg(rgba(SEPARATOR))
            .border_t(px(0.5))
            .border_color(rgba(SEPARATOR))
            .key_context(keys::CONTEXT)
            .track_focus(&self.term_focus)
            .on_key_down(cx.listener(Self::on_term_key))
            .children(out)
    }

    pub fn pane(&mut self, id: &str, n: Option<usize>, m: &'static Metrics, cx: &mut Context<Self>) -> Div {
        let focused = self.focused.as_deref() == Some(id);
        let exit = self.sessions.get(id).and_then(|s| s.exit);
        let known = self.sessions.get(id).is_some();
        let title = match self.summary(id) {
            Some(a) => format!("{} — {}", a.provider, basename(&a.cwd)),
            None => self.pane_label(id),
        };
        let banner = self.summary(id).and_then(status::banner).map(|text| {
            div().flex_none().px(px(16.)).py(px(6.)).border_b(px(0.5)).border_color(rgba(SEPARATOR)).bg(rgba(FILL_2)).text_size(px(12.)).text_color(rgba(TEXT_2)).child(text)
        });
        let body = match self.sessions.get_mut(id).and_then(|s| s.term.as_mut()) {
            Some(t) => {
                let (f, cells) = t.frame();
                termview::screen(&f, cells, m)
            }
            None => div().text_color(rgba(TEXT_3)).child(if known { "Connecting…" } else { "This session is not running." }),
        };
        let close_id = id.to_string();
        let header = n.map(|n| {
            div()
                .h(px(30.))
                .flex_none()
                .pl(px(16.))
                .pr(px(6.))
                .flex()
                .items_center()
                .border_b(px(0.5))
                .border_color(rgba(SEPARATOR))
                .text_color(rgba(if focused { TEXT } else { TEXT_3 }))
                .font_family(MONO)
                .text_size(px(11.5))
                .child(div().flex_1().truncate().child(format!("{n} · {title}")))
                .child(icon_button_sized(self::id(format!("close-{close_id}")), "x", 22., TEXT_3).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    this.close_pane(&close_id, cx);
                })))
        });
        let focus_id = id.to_string();
        let screen = div()
            .relative()
            .size_full()
            .overflow_hidden()
            .child(termview::surface(cx.entity(), id.to_string(), m, focused.then(|| self.term_focus.clone())))
            .child(body);
        div()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(rgba(SURFACE_SUNKEN))
            .when(focused && n.is_some(), |d| d.shadow(vec![BoxShadow { inset: true, ..ui::ring(SEPARATOR_STRONG, 0.5) }]))
            .overflow_hidden()
            .on_mouse_down(MouseButton::Left, cx.listener(move |this, _: &MouseDownEvent, window, cx| this.focus_pane(focus_id.clone(), window, cx)))
            .children(header)
            .children(banner)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .pt(px(10.))
                    .px(px(20.))
                    .pb(px(14.))
                    .font_family(MONO)
                    .text_size(px(m.size))
                    .line_height(px(m.line))
                    .text_color(rgba(TEXT))
                    .child(screen),
            )
            .children(exit.map(|c| {
                div().px(px(24.)).pb(px(12.)).text_size(px(12.)).text_color(rgba(if c == 0 { TEXT_3 } else { FAILED })).child(format!("Process exited with code {c}"))
            }))
    }
}

impl Render for Desktop {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_code(window, cx);
        self.sync_view(window, cx);
        let lead = match self.layout {
            Layout::Sidebars => Some(self.aside(cx)),
            Layout::Compact => Some(self.nav(cx)),
            Layout::Focus => None,
        };
        let column = (self.layout == Layout::Sidebars).then(|| self.column_view(cx));
        let panel = (self.layout == Layout::Compact && self.panel).then(|| self.panel_view(cx));
        let page = self.main_view(cx);
        let overlay = self.overlay_view(window, cx);
        div()
            .relative()
            .size_full()
            .flex()
            .bg(rgba(WINDOW))
            .font_family(SANS)
            .line_height(relative(1.2))
            .text_color(rgba(TEXT))
            .track_focus(&self.root)
            .capture_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                if ev.keystroke.key != "escape" {
                    return;
                }
                if this.close_picker() || this.close_menus() {
                    cx.notify();
                } else if this.overlay.is_some_and(|o| o != Overlay::Palette) {
                    this.close_overlay(window, cx);
                } else {
                    return;
                }
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &crate::OpenPalette, window, cx| this.open(Overlay::Palette, window, cx)))
            .on_action(cx.listener(|this, _: &crate::StartSession, window, cx| this.open(Overlay::NewSession, window, cx)))
            .on_action(cx.listener(Self::go_to_file))
            .on_action(cx.listener(Self::new_worktree))
            .on_action(cx.listener(Self::project_settings))
            .on_action(cx.listener(Self::next_waiting))
            .on_action(cx.listener(Self::toggle_rail))
            .on_action(cx.listener(Self::toggle_focus))
            .on_action(cx.listener(Self::new_tab))
            .on_action(cx.listener(Self::open_selected))
            .children(lead)
            .children(column)
            .child(page)
            .children(panel)
            .children(overlay)
    }
}

#[cfg(test)]
mod tests {
    use super::{Info, ago, ago_long, command_line, initials, tilde};

    #[test]
    fn formats_times_like_the_design() {
        let min = 60_000;
        assert_eq!(ago(0, min), "");
        assert_eq!(ago(1, 30_000), "now");
        assert_eq!(ago(1, 1 + 22 * min), "22m");
        assert_eq!(ago(1, 1 + 3 * 60 * min + 5 * min), "3h");
        assert_eq!(ago(1, 1 + 30 * 60 * min), "yesterday");
        assert_eq!(ago(1, 1 + 72 * 60 * min), "3d");
        assert_eq!(ago_long(1, 1 + 14 * min), "14m ago");
        assert_eq!(ago_long(1, 30_000), "now");
    }

    #[test]
    fn login_shells_show_as_their_name() {
        let info = Info { cmd: "/bin/zsh".into(), args: vec!["-l".into()], ..Default::default() };
        assert_eq!(command_line(&info), "zsh");
        let info = Info { cmd: "/opt/homebrew/bin/fish".into(), args: ["-l", "-c", "$argv; exec fish -l", "claude"].map(String::from).to_vec(), ..Default::default() };
        assert_eq!(command_line(&info), "fish");
        let info = Info { cmd: "pnpm".into(), args: vec!["dev".into(), "--port".into(), "8081".into()], ..Default::default() };
        assert_eq!(command_line(&info), "pnpm dev --port 8081");
    }

    #[test]
    fn initials_come_from_the_last_word() {
        assert_eq!(initials("app-android"), "AN");
        assert_eq!(initials("cs"), "CS");
        assert_eq!(initials("shared-ui"), "UI");
    }

    #[test]
    fn tilde_shortens_home_only() {
        let home = std::env::var("HOME").unwrap();
        assert_eq!(tilde(&format!("{home}/code/app")), "~/code/app");
        assert_eq!(tilde(&format!("{home}x/app")), format!("{home}x/app"));
    }
}
