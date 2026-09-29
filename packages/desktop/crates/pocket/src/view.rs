use daemon::Info;
use crate::termview::{self, Metrics};
use ui::{self, Segment, State, dot, icon_button_sized};
use theme::*;
use workspace::Tab;
use crate::status::{self, Kind, SECTIONS, section};
use crate::{Card, Desktop, Layout, Overlay, Screen, Side, Status};
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

pub fn column() -> Div {
    ui::side(div().w(px(348.)).flex_none().h_full().flex().flex_col().overflow_hidden())
}

pub fn empty(text: impl Into<SharedString>) -> Div {
    div().p(px(16.)).text_size(px(13.5)).text_color(rgba(TEXT_3)).child(text.into())
}

fn today(ms: i64) -> bool {
    let local = |t: chrono::DateTime<chrono::Utc>| t.with_timezone(&chrono::Local).date_naive();
    chrono::DateTime::from_timestamp_millis(ms).is_some_and(|t| local(t) == chrono::Local::now().date_naive())
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
        self.overlay = Some(o);
        match o {
            Overlay::Palette => self.open_palette(window, cx),
            Overlay::NewSession => self.reset_new_form(None, false, window, cx),
            Overlay::AddRepo => self.reset_repo_form(None, window, cx),
            Overlay::ProjectMenu | Overlay::More => {}
        }
        cx.notify();
    }

    pub fn close_overlay(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.overlay = None;
        window.focus(&self.root, cx);
        cx.notify();
    }

    fn rail(&self, cx: &mut Context<Self>) -> Div {
        let open = self.rail_open;
        let toggle = icon_button_sized("rail-toggle", if open { "sidebar-collapse" } else { "sidebar-expand" }, 30., TEXT_2)
            .rounded(px(7.))
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.toggle_rail(&crate::ToggleRail, window, cx)));
        let rule = div().h(px(0.5)).flex_none().bg(rgba(SEPARATOR_STRONG));
        let add = cx.listener(|this, _: &ClickEvent, window, cx| this.open(Overlay::AddRepo, window, cx));
        let inbox = cx.listener(|this, _: &ClickEvent, window, cx| this.open_inbox(window, cx));
        let notes = crate::inbox::count(&self.agents);
        let me = if self.initials.is_empty() { "ME".to_string() } else { self.initials.clone() };
        let rail = drag_area(ui::side(div())).w(px(if open { 240. } else { 72. })).flex_none().h_full().pb(px(12.)).flex().flex_col();
        if !open {
            let tiles = self.projects().into_iter().enumerate().map(|(i, p)| {
                let selected = self.screen == Screen::Sessions && self.project.as_ref() == Some(&p);
                let tile = ui::repo_tile(&initials(&self.repo_name(&p)), 38., selected, self.project_state(&p));
                div().id(("rail-project", i)).cursor_pointer().child(tile).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.select_project(p.clone(), cx)))
            });
            let bell = icon_button_sized("rail-bell", "bell", 38., if self.screen == Screen::Inbox { TEXT } else { TEXT_2 })
                .relative()
                .rounded(px(11.))
                .when(notes > 0, |d| d.child(ui::count_badge(notes)))
                .on_click(inbox);
            return rail
                .pt(px(37.))
                .items_center()
                .gap(px(12.))
                .child(toggle)
                .child(rule.w(px(26.)).mt(px(4.)).mb(px(2.)))
                .children(tiles)
                .child(ui::add_tile("rail-add", 38.).on_click(add))
                .child(div().flex_1())
                .child(bell)
                .child(ui::avatar(&me, 32.));
        }
        let row = |id: ElementId| div().id(id).h(px(40.)).px(px(8.)).flex().flex_none().items_center().gap(px(10.)).rounded(px(10.)).cursor_pointer().hover(|s| s.bg(rgba(FILL_2)));
        let rows = self.projects().into_iter().enumerate().map(|(i, p)| {
            let selected = self.screen == Screen::Sessions && self.project.as_ref() == Some(&p);
            let roll = self.roll_up(&p);
            let note = roll.map(|r| {
                let color = match r.0 {
                    Status::NeedsYou => WAITING_TEXT,
                    Status::Failed => FAILED,
                    Status::Done => ACCENT,
                    _ => RUNNING_TEXT,
                };
                (status::roll_up_label(r), color)
            });
            row(("rail-row", i).into())
                .when(selected, |d| d.bg(rgba(FILL_3)))
                .child(ui::repo_tile(&initials(&self.repo_name(&p)), 28., selected, roll.map(|(s, _)| state(s, 0, 0))))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_size(px(13.5))
                        .font_weight(if selected { FontWeight::SEMIBOLD } else { FontWeight::MEDIUM })
                        .child(self.repo_name(&p)),
                )
                .children(note.map(|(text, color)| div().flex_none().text_size(px(11.5)).font_weight(FontWeight::MEDIUM).text_color(rgba(color)).child(text)))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.select_project(p.clone(), cx)))
        });
        rail.pt(px(6.))
            .px(px(10.))
            .gap(px(2.))
            .child(div().h(px(30.)).flex().flex_none().justify_end().child(toggle))
            .child(rule.mt(px(10.)).mx(px(4.)).mb(px(8.)))
            .child(div().pt(px(4.)).px(px(8.)).pb(px(6.)).text_size(px(12.)).font_weight(FontWeight::SEMIBOLD).text_color(rgba(TEXT_3)).child("Projects"))
            .children(rows)
            .child(
                row("rail-add".into())
                    .text_color(rgba(TEXT_2))
                    .child(ui::add_tile("rail-add-tile", 28.))
                    .child(div().text_size(px(13.5)).font_weight(FontWeight::MEDIUM).child("Add project"))
                    .on_click(add),
            )
            .child(div().flex_1())
            .child(
                div()
                    .id("rail-bell")
                    .h(px(38.))
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(10.))
                    .text_size(px(13.5))
                    .font_weight(FontWeight::MEDIUM)
                    .pl(px(12.))
                    .pr(px(8.))
                    .rounded(px(10.))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgba(FILL_2)))
                    .child(icon("bell", 17., TEXT))
                    .child(div().flex_1().pl(px(3.)).child("Notifications"))
                    .when(notes > 0, |d| d.child(ui::count_badge(notes).relative().top_0().right_0()))
                    .on_click(inbox),
            )
            .child(div().h(px(44.)).pl(px(6.)).pr(px(8.)).flex().flex_none().items_center().gap(px(10.)).child(ui::avatar(&me, 32.)).child(div().text_size(px(13.5)).font_weight(FontWeight::MEDIUM).child("Account")))
    }

    /// The expanded sidebar: repositories with their worktrees and each worktree's agents.
    fn aside(&self, cx: &mut Context<Self>) -> Div {
        let top = drag_area(div())
            .h(px(46.))
            .flex()
            .flex_none()
            .items_center()
            .justify_end()
            .gap(px(2.))
            .child(icon_button_sized("aside-bell", "bell", 28., TEXT_3).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open_inbox(window, cx))))
            .child(icon_button_sized("aside-toggle", "sidebar", 28., TEXT_3).on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                this.toggle_sidebar(&crate::ToggleSidebar, window, cx)
            })));
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
            let selected = self.screen == Screen::Sessions && self.project.as_ref() == Some(&p);
            let (count, state) = if selected { (self.worktrees.get(&p).map(Vec::len), None) } else { (None, self.project_state(&p)) };
            let target = p.clone();
            repos.push(
                ui::repo_row(("aside-repo", i), &self.repo_name(&p), selected, count, state, ("aside-spin", i))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.select_project(target.clone(), cx)))
                    .into_any_element(),
            );
            if selected {
                repos.extend(self.worktree_rows(&p, cx));
            }
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
        ui::side(div())
            .w(px(272.))
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
            .child(foot)
    }

    /// A worktree's sessions, the one needing you first.
    fn tree_cards(&self, project: &str, tree: &str) -> Vec<Card> {
        let mut out: Vec<Card> = self.cards(project).into_iter().filter(|c| self.worktree_of(&c.cwd).is_some_and(|w| w.path == tree)).collect();
        out.sort_by_key(|c| c.status != Status::NeedsYou);
        out
    }

    fn worktree_rows(&self, project: &str, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mut out = Vec::new();
        let mut trees: Vec<(git::Worktree, Vec<Card>)> = self.worktrees.get(project).cloned().unwrap_or_default().into_iter().map(|w| {
            let cards = self.tree_cards(project, &w.path);
            (w, cards)
        }).collect();
        trees.sort_by_key(|(w, cards)| (!w.main, std::cmp::Reverse(cards.iter().map(|c| c.at).max())));
        let current = self.cwd();
        for (i, (w, mine)) in trees.into_iter().enumerate() {
            let label = if w.main { "main".to_string() } else { w.branch.clone() };
            let state = if !w.main && self.merged.contains(&w.branch) {
                Some(State::Merged)
            } else {
                status::roll_up(mine.iter().map(|c| c.status)).map(|(s, _)| state(s, 0, 0))
            };
            let selected = current.as_ref() == Some(&w.path);
            let path = w.path.clone();
            out.push(
                ui::worktree_row(("aside-tree", i), label, tilde(&w.path), w.main, selected, state)
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.worktree = Some(path.clone());
                        this.session = None;
                        cx.notify();
                    }))
                    .into_any_element(),
            );
            let mut counts: Vec<(&str, usize)> = Vec::new();
            for c in mine.iter().filter(|c| matches!(c.status, Status::NeedsYou | Status::Working)) {
                match counts.iter_mut().find(|(p, _)| *p == c.provider) {
                    Some((_, n)) => *n += 1,
                    None => counts.push((&c.provider, 1)),
                }
            }
            if !w.main && !counts.is_empty() {
                out.push(
                    div()
                        .pt(px(2.))
                        .pb(px(4.5))
                        .pl(px(58.))
                        .flex()
                        .gap(px(4.))
                        .children(counts.into_iter().map(|(p, n)| ui::agent_badge(p, n.to_string())))
                        .into_any_element(),
                );
            }
        }
        out.push(
            div()
                .id("aside-new-worktree")
                .h(px(30.))
                .pl(px(34.))
                .flex()
                .flex_none()
                .items_center()
                .gap(px(10.))
                .rounded(px(9.))
                .cursor_pointer()
                .text_size(px(13.))
                .text_color(rgba(TEXT_4))
                .hover(|s| s.bg(rgba(FILL_2)))
                .child(div().w(px(14.)).flex().justify_center().child(icon("plus", 12., TEXT_4)))
                .child("New worktree")
                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.new_worktree(&crate::NewWorktree, window, cx)))
                .into_any_element(),
        );
        out
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
            (Screen::Inbox, _) => return self.inbox_list(cx),
            (_, Side::Sessions) => self.session_list(cx).into_any_element(),
            (_, Side::Explorer) => self.explorer(cx).into_any_element(),
            (_, Side::Changes) => self.changes_list(cx).into_any_element(),
        };
        let changes = self.repo().map_or(0, |r| r.files.len());
        let tabs = ui::segmented(
            vec![
                Segment { icon: None, value: Side::Sessions, label: "Sessions".into(), badge: None },
                Segment { icon: None, value: Side::Explorer, label: "Explore".into(), badge: None },
                Segment { icon: None, value: Side::Changes, label: "Changes".into(), badge: (changes > 0).then(|| changes.to_string()) },
            ],
            self.side,
            false,
            true,
            |this, side, cx| {
                this.side = side;
                if side == Side::Changes {
                    this.load_diff(cx);
                }
                cx.notify();
            },
            cx,
        );
        column().when(self.wide, |d| d.w(px(334.))).child(self.column_header(cx)).child(div().px(px(14.)).pb(px(10.)).child(tabs)).child(body)
    }

    fn column_header(&self, cx: &mut Context<Self>) -> Div {
        let project = self.project.clone().unwrap_or_default();
        let name = if project.is_empty() { "No project".to_string() } else { self.repo_name(&project) };
        let compose = icon_button_sized("column-compose", "compose", 30., TEXT_2)
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open(Overlay::NewSession, window, cx)));
        let head = drag_area(div()).pt(px(16.)).pr(px(14.)).pb(px(10.)).pl(px(18.)).flex().flex_none().items_start().gap(px(8.));
        if !self.wide {
            let title = div()
                .id("project-name")
                .flex()
                .min_w_0()
                .items_center()
                .gap(px(5.))
                .cursor_pointer()
                .child(div().truncate().text_size(px(17.)).line_height(px(20.)).font_weight(FontWeight::BOLD).child(name))
                .child(icon("chevron-down", 12., TEXT_4))
                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open(Overlay::ProjectMenu, window, cx)));
            let path = div().truncate().font_family(MONO).text_size(px(11.5)).line_height(px(15.)).text_color(rgba(TEXT_3)).child(tilde(&project));
            let search = icon_button_sized("column-search", "search", 30., TEXT_2)
                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open(Overlay::Palette, window, cx)));
            return head
                .items_center()
                .gap(px(10.))
                .child(div().flex_1().min_w_0().flex().flex_col().gap(px(1.)).child(title).child(path))
                .child(search)
                .child(compose);
        }
        let tree = self.cwd().and_then(|t| self.worktrees.values().flatten().find(|w| w.path == t)).cloned();
        let repo = tree.as_ref().and_then(|t| self.repos.get(&t.path)).or_else(|| self.repos.get(&project));
        let title = match &tree {
            Some(t) if t.main => "main".to_string(),
            Some(t) => t.branch.clone(),
            None => name.clone(),
        };
        let letter: String = initials(&name).chars().take(1).collect();
        let crumb = div()
            .flex()
            .items_center()
            .gap(px(6.))
            .text_size(px(12.))
            .line_height(px(17.))
            .text_color(rgba(TEXT_3))
            .child(ui::repo_tile(&letter, 16., false, None))
            .child(name)
            .child(div().text_color(rgba(TEXT_6)).child("/"))
            .child(if tree.as_ref().is_some_and(|t| !t.main) { "worktree" } else { "main" });
        let branch = repo.map(|r| {
            div()
                .flex()
                .items_center()
                .gap(px(5.))
                .font_family(MONO)
                .text_size(px(11.))
                .line_height(px(14.))
                .text_color(rgba(TEXT_3))
                .child(icon("branch", 11., TEXT_3))
                .child(r.branch.clone())
                .child(div().text_color(rgba(TEXT_6)).child("·"))
                .child(format!("↑{} ↓{}", r.ahead, r.behind))
        });
        let more = icon_button_sized("column-more", "more", 30., TEXT_2)
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open(Overlay::ProjectMenu, window, cx)));
        head.child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(3.))
                .child(crumb)
                .child(div().truncate().text_size(px(16.5)).line_height(px(19.)).font_weight(FontWeight::BOLD).child(title))
                .children(branch),
        )
        .child(compose)
        .child(more)
    }

    fn session_list(&mut self, cx: &mut Context<Self>) -> Stateful<Div> {
        let start = ui::trigger_field("start-session", "sparkle", "Start a new session…", "⌘N")
            .mx(px(14.))
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open(Overlay::NewSession, window, cx)));
        let list = div().id("cards").flex_1().min_h_0().overflow_y_scroll().flex().flex_col().when(!self.wide, |d| d.child(start));
        let Some(project) = self.project.clone() else {
            return list.child(empty("Add a project with + to start."));
        };
        let tree = self.cwd();
        let mut cards: Vec<Card> = self
            .cards(&project)
            .into_iter()
            .filter(|c| self.tree_of(&c.cwd) == tree)
            .collect();
        cards.sort_by_key(|c| c.status);
        let mut body = div().pt(px(2.)).px(px(8.)).pb(px(8.)).flex().flex_col().gap(px(2.));
        let mut i = 0;
        for (g, label) in SECTIONS.into_iter().enumerate() {
            let mine: Vec<Card> = cards
                .iter()
                // The design's worktree view keeps finished sessions under one heading.
                .filter(|c| section(c.status, !self.wide && today(c.at)) == g)
                .cloned()
                .collect();
            if mine.is_empty() {
                continue;
            }
            body = body.child(ui::section_header(label, Some(mine.len())));
            for c in mine {
                body = body.child(self.card(i, c, cx));
                i += 1;
            }
        }
        list.child(body)
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
        ui::session_row(("card", i), selected, c.title, Some(pill), lead)
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
            Layout::Sidebars if self.wide => (24., None),
            Layout::Sidebars => (14., Some(toggle("sidebar-collapse", cx))),
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
            Layout::Sidebars if self.wide => Some(self.aside(cx)),
            Layout::Sidebars => Some(self.rail(cx)),
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
                if this.close_picker() || std::mem::take(&mut this.tab_menu) {
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
            .on_action(cx.listener(Self::toggle_sidebar))
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
