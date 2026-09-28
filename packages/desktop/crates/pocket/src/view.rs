use daemon::Info;
use crate::termview::{self, Metrics};
use ui::{self, Segment, State, dot, icon_button_sized};
use theme::*;
use workspace::Tab;
use crate::{Card, Desktop, Overlay, Screen, Side, Status};
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
pub fn command_line(info: &Info) -> String {
    let args = info.args.iter().filter(|a| *a != "-l").cloned();
    std::iter::once(basename(&info.cmd)).chain(args).collect::<Vec<_>>().join(" ")
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
        Status::NeedsYou => State::Waiting,
        Status::Working => State::Running,
        Status::Failed => State::Failed,
        Status::Done => State::Done(added, removed),
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

    pub fn project_state(&self, p: &str) -> Option<State> {
        let cards = self.cards(p);
        if cards.iter().any(|c| c.status == Status::NeedsYou) {
            Some(State::Waiting)
        } else if cards.iter().any(|c| c.status == Status::Working) {
            Some(State::Running)
        } else {
            None
        }
    }

    pub fn open(&mut self, o: Overlay, window: &mut Window, cx: &mut Context<Self>) {
        self.overlay = Some(o);
        match o {
            Overlay::Palette => self.open_palette(window, cx),
            Overlay::NewSession => self.reset_new_form(None, window, cx),
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
        let asks = self.agents.pending.len();
        let me = if self.initials.is_empty() { "ME".to_string() } else { self.initials.clone() };
        let rail = drag_area(ui::side(div())).w(px(if open { 240. } else { 72. })).flex_none().h_full().pb(px(12.)).flex().flex_col();
        if !open {
            let tiles = self.projects().into_iter().enumerate().map(|(i, p)| {
                let selected = self.screen == Screen::Sessions && self.project.as_ref() == Some(&p);
                let state = self.project_state(&p);
                let tile = ui::repo_tile(&initials(&self.repo_name(&p)), 38., selected, state == Some(State::Waiting), state == Some(State::Running));
                div().id(("rail-project", i)).cursor_pointer().child(tile).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.select_project(p.clone(), cx)))
            });
            let bell = icon_button_sized("rail-bell", "bell", 38., if self.screen == Screen::Inbox { TEXT } else { TEXT_2 })
                .relative()
                .rounded(px(11.))
                .when(asks > 0, |d| d.child(ui::count_badge(asks)))
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
            let state = self.project_state(&p);
            let waiting = self.cards(&p).iter().filter(|c| c.status == Status::NeedsYou).count();
            let note = match state {
                Some(State::Waiting) => Some((format!("{waiting} waiting"), WAITING_TEXT)),
                Some(State::Running) => Some(("Running".to_string(), RUNNING_TEXT)),
                _ => None,
            };
            row(("rail-row", i).into())
                .when(selected, |d| d.bg(rgba(FILL_3)))
                .child(ui::repo_tile(&initials(&self.repo_name(&p)), 28., selected, state == Some(State::Waiting), state == Some(State::Running)))
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
                    .child(div().text_size(px(13.5)).font_weight(FontWeight::MEDIUM).child("Add repository"))
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
                    .when(asks > 0, |d| d.child(ui::count_badge(asks).relative().top_0().right_0()))
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
            .h(px(32.))
            .rounded(px(10.))
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open(Overlay::Palette, window, cx)));
        let header = div()
            .pt(px(14.))
            .pb(px(6.))
            .pl(px(8.))
            .pr(px(4.))
            .flex()
            .items_center()
            .text_size(px(12.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(rgba(TEXT_3))
            .child(div().flex_1().child("Repositories"))
            .child(
                icon_button_sized("aside-add", "plus", 22., TEXT_3)
                    .rounded(px(6.))
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open(Overlay::AddRepo, window, cx))),
            );
        let mut repos = Vec::new();
        for (i, p) in self.projects().into_iter().enumerate() {
            let selected = self.screen == Screen::Sessions && self.project.as_ref() == Some(&p);
            let count = self.cards(&p).len();
            let target = p.clone();
            repos.push(
                ui::repo_row(("aside-repo", i), &self.repo_name(&p), selected, Some(count), self.project_state(&p), ("aside-spin", i))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.select_project(target.clone(), cx)))
                    .into_any_element(),
            );
            if selected {
                repos.extend(self.worktree_rows(&p, cx));
            }
        }
        let body = div().id("aside-repos").flex_1().min_h_0().overflow_y_scroll().flex().flex_col().gap(px(2.)).children(repos);
        let foot = div()
            .pt(px(8.))
            .flex()
            .items_center()
            .gap(px(8.))
            .child(
                ui::button("aside-add-repo", ui::Variant::Glass, Some("plus"), "Add repository")
                    .flex_1()
                    .h(px(34.))
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

    fn worktree_rows(&self, project: &str, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mut out = Vec::new();
        let cards = self.cards(project);
        for (i, w) in self.worktrees.get(project).cloned().unwrap_or_default().into_iter().enumerate() {
            let mine: Vec<&Card> = cards.iter().filter(|c| self.worktree_of(&c.cwd).is_some_and(|x| x.path == w.path)).collect();
            let label = if w.main { "main".to_string() } else { mine.first().map_or_else(|| w.branch.clone(), |c| c.title.clone()) };
            let branch = if w.main { tilde(&w.path) } else { w.branch.clone() };
            let state = if !w.main && self.merged.contains(&w.branch) {
                Some(State::Merged)
            } else if mine.iter().any(|c| c.status == Status::NeedsYou) {
                Some(State::Waiting)
            } else if mine.iter().any(|c| c.status == Status::Working) {
                Some(State::Running)
            } else {
                None
            };
            let selected = self.worktree.as_ref() == Some(&w.path);
            let path = w.path.clone();
            out.push(
                ui::worktree_row(("aside-tree", i), label, branch, w.main, selected, state)
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.worktree = if this.worktree.as_ref() == Some(&path) { None } else { Some(path.clone()) };
                        this.session = None;
                        cx.notify();
                    }))
                    .into_any_element(),
            );
            let mut counts: Vec<(&str, usize)> = Vec::new();
            for c in &mine {
                match counts.iter_mut().find(|(p, _)| *p == c.provider) {
                    Some((_, n)) => *n += 1,
                    None => counts.push((&c.provider, 1)),
                }
            }
            if !w.main && !counts.is_empty() {
                out.push(
                    div()
                        .pb(px(4.))
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
    fn usage_card(&self) -> Option<Div> {
        let latest = |p: &str| {
            let a = self.agents.list.iter().filter(|a| a.provider == p && a.status != "closed").max_by_key(|a| a.updated_at)?;
            self.agents.context_left(&a.id)
        };
        let parts: Vec<Div> = ["claude", "codex"]
            .into_iter()
            .filter_map(|p| latest(p).map(|left| div().flex().items_center().gap(px(5.)).child(dot(7., provider_color(p))).child(format!("{left}%"))))
            .collect();
        (!parts.is_empty()).then(|| {
            div()
                .mt(px(8.))
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
        column().when(self.wide, |d| d.w(px(334.))).child(self.column_header(cx)).child(div().px(px(14.)).pb(px(12.)).child(tabs)).child(body)
    }

    fn column_header(&self, cx: &mut Context<Self>) -> Div {
        let project = self.project.clone().unwrap_or_default();
        let name = if project.is_empty() { "No project".to_string() } else { self.repo_name(&project) };
        let compose = icon_button_sized("column-compose", "compose", 30., TEXT_2)
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open(Overlay::NewSession, window, cx)));
        let head = drag_area(div()).pt(px(16.)).pr(px(12.)).pb(px(12.)).pl(px(18.)).flex().flex_none().items_start().gap(px(8.));
        if !self.wide {
            let title = div()
                .id("project-name")
                .flex()
                .min_w_0()
                .items_center()
                .gap(px(5.))
                .cursor_pointer()
                .child(div().truncate().text_size(px(17.)).font_weight(FontWeight::BOLD).child(name))
                .child(icon("chevron-down", 12., TEXT_4))
                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open(Overlay::ProjectMenu, window, cx)));
            let path = div().truncate().font_family(MONO).text_size(px(11.5)).text_color(rgba(TEXT_3)).child(tilde(&project));
            let search = icon_button_sized("column-search", "search", 30., TEXT_2)
                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open(Overlay::Palette, window, cx)));
            return head
                .child(div().flex_1().min_w_0().flex().flex_col().gap(px(2.)).child(title).child(path))
                .child(div().flex().flex_none().gap(px(2.)).child(search).child(compose));
        }
        let tree = self.worktree.as_deref().and_then(|t| self.worktrees.values().flatten().find(|w| w.path == t)).cloned();
        let repo = tree.as_ref().and_then(|t| self.repos.get(&t.path)).or_else(|| self.repos.get(&project));
        let title = match &tree {
            Some(t) if !t.main => {
                let card = self.cards(&project).into_iter().find(|c| self.worktree_of(&c.cwd).is_some_and(|w| w.path == t.path));
                card.map_or_else(|| t.branch.clone(), |c| c.title)
            }
            _ => name.clone(),
        };
        let letter: String = initials(&name).chars().take(1).collect();
        let crumb = div()
            .flex()
            .items_center()
            .gap(px(6.))
            .text_size(px(12.5))
            .text_color(rgba(TEXT_3))
            .child(ui::repo_tile(&letter, 16., false, false, false))
            .child(name)
            .child(div().text_color(rgba(TEXT_6)).child("/"))
            .child(if tree.as_ref().is_some_and(|t| !t.main) { "worktree" } else { "main" });
        let branch = repo.map(|r| {
            div()
                .flex()
                .items_center()
                .gap(px(6.))
                .font_family(MONO)
                .text_size(px(11.))
                .text_color(rgba(TEXT_3))
                .child(icon("branch", 11., TEXT_4))
                .child(r.branch.clone())
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
                .child(div().truncate().text_size(px(16.5)).font_weight(FontWeight::BOLD).child(title))
                .children(branch),
        )
        .child(div().flex().flex_none().gap(px(2.)).child(compose).child(more))
    }

    fn session_list(&mut self, cx: &mut Context<Self>) -> Stateful<Div> {
        let start = ui::trigger_field("start-session", "sparkle", "Start a new session…", "⌘N")
            .mx(px(14.))
            .mb(px(4.))
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open(Overlay::NewSession, window, cx)));
        let list = div().id("cards").flex_1().min_h_0().overflow_y_scroll().flex().flex_col().child(start);
        let Some(project) = self.project.clone() else {
            return list.child(empty("Add a repository with + to start."));
        };
        let now = now_ms();
        let tree = self.worktree.clone();
        let cards: Vec<Card> = self
            .cards(&project)
            .into_iter()
            .filter(|c| tree.as_ref().is_none_or(|t| self.worktree_of(&c.cwd).is_some_and(|w| &w.path == t)))
            .collect();
        let group = |c: &Card| match c.status {
            Status::NeedsYou => 0,
            Status::Working => 1,
            _ if today(c.at) => 2,
            _ => 3,
        };
        let mut body = div().px(px(8.)).pb(px(8.)).flex().flex_col().gap(px(2.));
        let mut i = 0;
        for (g, label) in ["Needs you", "Running", "Earlier today", "Earlier"].into_iter().enumerate() {
            let mine: Vec<Card> = cards.iter().filter(|c| group(c) == g).cloned().collect();
            if mine.is_empty() {
                continue;
            }
            body = body.child(ui::section_header(label, Some(mine.len())));
            for c in mine {
                body = body.child(self.card(i, c, now, cx));
                i += 1;
            }
        }
        list.child(body)
    }

    fn card(&self, i: usize, c: Card, now: i64, cx: &mut Context<Self>) -> Stateful<Div> {
        let selected = self.session.as_ref() == Some(&c.id);
        let repo = self.repos.get(&c.cwd);
        let branch = repo.map(|r| r.branch.clone()).unwrap_or_default();
        let (added, removed) = repo.map(|r| r.totals()).unwrap_or_default();
        let tags = self.store.children_of(&c.id).map(|child| self.pane_label(child)).collect();
        let id = c.id.clone();
        ui::session_row(("card", i), selected, c.title, state(c.status, added, removed), &c.provider, branch, ago(c.at, now), tags)
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.select_session(id.clone(), window, cx)))
    }

    fn main_view(&mut self, cx: &mut Context<Self>) -> Div {
        let body = match (self.screen, self.side) {
            (Screen::Inbox, _) => self.inbox_detail(cx),
            (_, Side::Changes) if self.diff_file.is_some() => self.diff_view(cx),
            (_, Side::Explorer) if self.file.is_some() => self.file_view(cx),
            _ => match self.session.clone() {
                Some(id) => self.session_page(&id, cx),
                None => self.blank_page(cx),
            },
        };
        ui::page(div()).flex_1().min_w_0().h_full().flex().flex_col().overflow_hidden().child(body)
    }

    fn blank_page(&self, cx: &mut Context<Self>) -> Div {
        let text = if self.project.is_none() { "Add a repository to begin." } else { "Pick a session, or start a new one." };
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
            (None, Some(s)) => command_line(&s.info),
            (None, None) => "session".into(),
        }
    }

    /// The page's top bar: sidebar toggle, breadcrumb and meta on the left, `right` on the far side.
    pub fn page_bar(&self, crumbs: Vec<String>, meta: Vec<AnyElement>, right: impl IntoElement, cx: &mut Context<Self>) -> Div {
        let toggle = (self.focus || !self.wide).then(|| {
            icon_button_sized("focus-toggle", if self.focus { "sidebar-expand" } else { "sidebar-collapse" }, 28., TEXT_2)
                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.toggle_focus(&crate::ToggleFocus, window, cx)))
        });
        drag_area(ui::page_bar())
            .when(self.wide, |d| d.pl(px(24.)))
            // Leaves room for the window's traffic lights once the sidebars are hidden.
            .when(self.focus, |d| d.pl(px(82.)))
            .children(toggle)
            .child(ui::breadcrumb(crumbs))
            .child(ui::meta_row(meta))
            .child(div().ml_auto().flex().flex_none().items_center().gap(px(8.)).child(right))
    }

    fn session_page(&mut self, id: &str, cx: &mut Context<Self>) -> Div {
        let now = now_ms();
        let summary = self.summary(id).cloned();
        let title = summary.as_ref().map(|a| a.title.clone()).filter(|t| !t.is_empty()).unwrap_or_else(|| self.pane_label(id));
        let repo = self.repo().cloned();
        let (added, removed) = repo.as_ref().map(|r| r.totals()).unwrap_or_default();
        let compact = self.wide;
        let mut meta = Vec::new();
        if let Some(a) = &summary {
            let agent = format!("{} · {}", provider_name(&a.provider), agents::model_label(a));
            let agent = if compact { div().child(agent) } else { ui::meta_value(agent) };
            meta.push(ui::meta_item().child(dot(7., provider_color(&a.provider))).child(agent).into_any_element());
        }
        if let Some(r) = repo.as_ref().filter(|_| !compact) {
            meta.push(ui::meta_item().child(icon("branch", 13., TEXT_2)).child(ui::meta_value(r.branch.clone())).into_any_element());
        }
        if let Some(a) = &summary {
            let clock = ui::meta_item().child(icon("clock", 13., TEXT_2));
            meta.push(if compact { clock.gap(px(5.)).child(ago(a.created_at, now)) } else { clock.child(ui::meta_value(ago_long(a.created_at, now))) }.into_any_element());
            if let Some(left) = self.agents.context_left(&a.id).filter(|_| !compact) {
                let bar = ui::context_bar(left as f32 / 100.);
                meta.push(ui::meta_item().gap(px(8.)).child("Context").child(bar).child(ui::meta_value(format!("{}%", 100 - left.min(100)))).into_any_element());
            }
        }
        if added + removed > 0 {
            meta.push(
                ui::meta_item()
                    .id("meta-diff")
                    .when(compact, |d| d.gap(px(5.)))
                    .cursor_pointer()
                    .child(icon("branch", 13., TEXT_2))
                    .child(ui::meta_diff(added, removed, 12.))
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.open_changes(None, cx)))
                    .into_any_element(),
            );
        }
        if let Some(e) = self.error.clone() {
            meta.push(div().truncate().text_size(px(12.5)).text_color(rgba(FAILED)).child(e).into_any_element());
        }
        let right = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .child(ui::icon_group([
                ui::group_button("split-right", "split-right").on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.new_shell(Some(false), cx))),
                ui::group_button("split-down", "split-down").on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.new_shell(Some(true), cx))),
            ]))
            .child(ui::icon_group([
                ui::group_button("session-more", "more").on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open(Overlay::More, window, cx))),
            ]));
        let crumbs = if self.wide { vec![self.project.as_deref().map(|p| self.repo_name(p)).unwrap_or_default(), title] } else { vec!["Sessions".into(), title] };
        let tabs = self.tab_strip(id, cx);
        let body = match self.workspace(id).active() {
            Some(Tab::Term(rows)) => {
                let rows = rows.clone();
                self.panes(rows, cx)
            }
            Some(Tab::Changes) => self.diff_box(cx),
            None => div().flex_1(),
        };
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(self.page_bar(crumbs, meta, right, cx))
            .children(tabs)
            .child(body)
    }

    fn tab_lead(&self, panes: Option<Vec<String>>) -> Div {
        let row = div().flex().items_center().gap(px(7.));
        let Some(p) = panes else {
            return row.child(icon("branch", 13., TEXT_3)).child("Changes");
        };
        let count = |label: String| if p.len() > 1 { format!("{label} · {} panes", p.len()) } else { label };
        if let Some(a) = self.summary(&p[0]) {
            return row
                .child(dot(7., provider_color(&a.provider)))
                .child(count(a.provider.clone()))
                .when(self.agents.needs_you(&p[0]), |d| d.child(dot(6., WAITING)));
        }
        let mark = match self.sessions.get(&p[0]).map(|s| s.exit) {
            Some(None) => dot(6., RUNNING).into_any_element(),
            Some(Some(c)) if c != 0 => icon("x", 12., FAILED).into_any_element(),
            _ => dot(6., TEXT_5).into_any_element(),
        };
        row.child(icon("terminal", 13., TEXT_3)).child(count(self.pane_label(&p[0]))).child(mark)
    }

    /// Shown only once a session has more than one tab.
    fn tab_strip(&mut self, parent: &str, cx: &mut Context<Self>) -> Option<Div> {
        let w = self.workspace(parent);
        if w.tabs.len() < 2 {
            return None;
        }
        let active = w.active;
        let tabs: Vec<Option<Vec<String>>> =
            w.tabs.iter().map(|t| if let Tab::Term(r) = t { Some(r.iter().flatten().cloned().collect()) } else { None }).collect();
        let items: Vec<_> = tabs
            .into_iter()
            .enumerate()
            .map(|(i, panes)| {
                let selected = i == active;
                div()
                    .id(("tab", i))
                    .h(px(28.))
                    .pl(px(12.))
                    .pr(px(4.))
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(8.))
                    .rounded(px(14.))
                    .cursor_pointer()
                    .font_family(MONO)
                    .text_size(px(12.))
                    .whitespace_nowrap()
                    .when(selected, |d| d.bg(rgba(FILL_3)).font_weight(FontWeight::SEMIBOLD).text_color(rgba(TEXT)))
                    .when(!selected, |d| d.font_weight(FontWeight::MEDIUM).text_color(rgba(TEXT_2)).hover(|s| s.bg(rgba(FILL_2))))
                    .child(self.tab_lead(panes))
                    .child(icon_button_sized(("close-tab", i), "x", 20., TEXT_3).rounded(px(10.)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        this.close_tab(i, cx);
                    })))
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.select_tab(i, window, cx)))
            })
            .collect();
        Some(
            div()
                .flex_none()
                .px(px(36.))
                .pb(px(10.))
                .flex()
                .items_center()
                .gap(px(2.))
                .children(items)
                .child(icon_button_sized("new-tab", "plus", 26., TEXT_3).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.new_shell(None, cx)))),
        )
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
        let lead = (!self.focus).then(|| if self.wide { self.aside(cx) } else { self.rail(cx) });
        let column = (!self.focus).then(|| self.column_view(cx));
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
                if ev.keystroke.key == "escape" && this.overlay.is_some_and(|o| o != Overlay::Palette) {
                    this.close_overlay(window, cx);
                    cx.stop_propagation();
                }
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
            .on_action(cx.listener(Self::open_selected))
            .children(lead)
            .children(column)
            .child(page)
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
    fn command_line_hides_the_login_flag() {
        let info = Info { cmd: "/bin/zsh".into(), args: vec!["-l".into()], ..Default::default() };
        assert_eq!(command_line(&info), "zsh");
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
