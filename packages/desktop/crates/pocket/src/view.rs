use daemon::Info;
use crate::termview::{self, Metrics};
use ui::{self, Segment, State, Variant, dot, icon_button, kbd};
use theme::*;
use workspace::Tab;
use crate::{Card, Desktop, Screen, Side, Status};
use gpui_kit::component::input::Input;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::path::{Path, PathBuf};

pub fn basename(path: &str) -> String {
    path.trim_end_matches('/').rsplit('/').next().unwrap_or_default().to_string()
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

/// "/bin/zsh -l" reads as "zsh": the login flag the desktop adds says nothing about the pane.
pub fn elapsed(ms: i64) -> String {
    let m = ms.max(0) / 60_000;
    if m < 60 { format!("{m}m") } else { format!("{}h {}m", m / 60, m % 60) }
}

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

fn state(status: Status, added: usize, removed: usize) -> State {
    match status {
        Status::NeedsYou => State::Waiting,
        Status::Working => State::Running,
        Status::Failed => State::Failed,
        Status::Done => State::Done(added, removed),
    }
}

fn nav_row(id: &'static str, name: &str, label: &str, selected: bool) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(34.))
        .pl(px(8.))
        .pr(px(8.))
        .flex()
        .flex_none()
        .items_center()
        .gap(px(9.))
        .rounded(px(9.))
        .cursor_pointer()
        .text_size(px(14.))
        .when(selected, |d| d.bg(rgba(ROW_SELECTED)).shadow(ui::row_shadow()).font_weight(FontWeight::SEMIBOLD))
        .when(!selected, |d| d.font_weight(FontWeight::MEDIUM).text_color(rgba(TEXT_BODY)).hover(|s| s.bg(rgba(FILL_2))))
        .child(icon(name, 16., TEXT_2))
        .child(div().flex_1().child(label.to_string()))
}

fn field() -> Div {
    div().h(px(32.)).px(px(10.)).flex().flex_none().items_center().gap(px(8.)).rounded(px(10.)).bg(rgba(FILL_3)).text_size(px(13.5)).text_color(rgba(TEXT_3))
}

pub fn column() -> Div {
    ui::side(div().w(px(332.)).flex_none().h_full().flex().flex_col().overflow_hidden())
}

pub fn empty(text: &'static str) -> Div {
    div().p(px(16.)).text_size(px(13.5)).text_color(rgba(TEXT_3)).child(text)
}

impl Desktop {
    fn rail(&self, cx: &mut Context<Self>) -> Div {
        let top = drag_area(div())
            .h(px(44.))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(1.))
            .child(div().flex_1())
            .child(icon_button("back", "back"))
            .child(icon_button("forward", "forward"))
            .child(icon_button("toggle-sidebar", "sidebar").on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.sidebar = !this.sidebar;
                cx.notify();
            })));
        let search = field()
            .id("rail-search")
            .cursor_pointer()
            .child(icon("search", 15., TEXT_3))
            .child(div().flex_1().child("Search"))
            .child(kbd("⌘ K"))
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.focus_search(&crate::FocusSearch, window, cx)));
        let asks = self.agents.pending.len();
        let inbox = self.screen == Screen::Inbox;
        let badge = div()
            .h(px(20.))
            .px(px(7.))
            .flex()
            .items_center()
            .rounded(px(10.))
            .bg(rgba(WAITING_BG))
            .text_size(px(11.5))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(rgba(WAITING_TEXT))
            .child(asks.to_string());
        let nav = div()
            .mt(px(10.))
            .flex()
            .flex_col()
            .gap(px(2.))
            .child(nav_row("inbox", "inbox", "Inbox", inbox).when(asks > 0, |d| d.child(badge)).on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                this.open_inbox(window, cx);
            })))
            .child(nav_row("automations", "bolt", "Automations", false));
        let header = div()
            .pt(px(18.))
            .pb(px(4.))
            .pl(px(8.))
            .flex()
            .items_center()
            .text_size(px(12.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(rgba(TEXT_3))
            .child(div().flex_1().child("Projects"))
            .child(icon_button("add-project", "plus").on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.add_project(cx))));
        let projects = self.projects().into_iter().enumerate().map(|(i, p)| {
            let selected = self.screen == Screen::Sessions && self.project.as_ref() == Some(&p);
            let cards = self.cards(&p);
            let state = if cards.iter().any(|c| c.status == Status::NeedsYou) {
                Some(State::Waiting)
            } else if cards.iter().any(|c| c.status == Status::Working) {
                Some(State::Running)
            } else {
                None
            };
            ui::repo_row(("project", i), &basename(&p), selected, state, ("project-spin", i))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.select_project(p.clone(), cx)))
        });
        ui::side(div())
            .w(px(248.))
            .flex_none()
            .h_full()
            .px(px(8.))
            .pb(px(8.))
            .flex()
            .flex_col()
            .child(top)
            .child(search)
            .child(nav)
            .child(header)
            .child(div().id("projects").flex_1().overflow_y_scroll().flex().flex_col().gap(px(2.)).children(projects))
            .child(nav_row("settings", "settings", "Settings", false).child(kbd("⌘ ,")))
    }

    fn sidebar_view(&mut self, cx: &mut Context<Self>) -> Div {
        let body = match (self.screen, self.side) {
            (Screen::Inbox, _) => return self.inbox_list(cx),
            (_, Side::Sessions) => self.session_list(cx).into_any_element(),
            (_, Side::Explorer) => self.explorer(cx).into_any_element(),
            (_, Side::Changes) => self.changes_list(cx).into_any_element(),
        };
        let name = self.project.as_deref().map(basename).unwrap_or_else(|| "No project".into());
        let changes = self.repo().map_or(0, |r| r.files.len());
        let header = drag_area(div())
            .h(px(52.))
            .pl(px(16.))
            .pr(px(10.))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(2.))
            .child(div().flex_1().truncate().text_size(px(17.)).font_weight(FontWeight::BOLD).child(name))
            .child(icon_button("find", "search").on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.focus_search(&crate::FocusSearch, window, cx))))
            .child(icon_button("new-session", "compose").on_click(cx.listener(|this, e: &ClickEvent, _, cx| this.new_session(if e.modifiers().alt { "codex" } else { "claude" }, cx))));
        let tabs = ui::segmented(
            vec![
                Segment { value: Side::Sessions, label: "Sessions".into(), badge: None },
                Segment { value: Side::Explorer, label: "Explore".into(), badge: None },
                Segment { value: Side::Changes, label: "Changes".into(), badge: (changes > 0).then(|| changes.to_string()) },
            ],
            self.side,
            false,
            true,
            |this, side, cx| {
                this.side = side;
                if side == Side::Changes {
                    this.refresh_git(cx);
                }
                cx.notify();
            },
            cx,
        );
        column().child(header).child(div().px(px(12.)).child(tabs)).child(body)
    }

    fn session_list(&mut self, cx: &mut Context<Self>) -> Div {
        let search = field()
            .mx(px(12.))
            .mt(px(10.))
            .pr(px(2.))
            .child(icon("search", 15., TEXT_3))
            .child(div().flex_1().child(Input::new(&self.filter).appearance(false).p_0().text_size(px(13.5))))
            .child(icon_button("filter", "filter"));
        let list = div().flex_1().min_h_0().flex().flex_col().child(search);
        let Some(project) = self.project.clone() else {
            return list.child(empty("Add a project with + to start."));
        };
        let query = self.filter.read(cx).value().to_lowercase();
        let now = now_ms();
        let cards = self.cards(&project).into_iter().filter(|c| query.is_empty() || c.title.to_lowercase().contains(&query));
        let cards: Vec<_> = cards.enumerate().map(|(i, c)| self.card(i, c, now, cx)).collect();
        list.child(div().id("cards").flex_1().overflow_y_scroll().p(px(8.)).flex().flex_col().gap(px(2.)).children(cards))
    }

    fn card(&self, i: usize, c: Card, now: i64, cx: &mut Context<Self>) -> Stateful<Div> {
        let selected = self.session.as_ref() == Some(&c.id);
        let repo = self.repos.get(&c.cwd);
        let branch = repo.map(|r| r.branch.clone()).unwrap_or_default();
        let (added, removed) = repo.map(|r| r.totals()).unwrap_or_default();
        let id = c.id.clone();
        ui::session_row(("card", i), selected, c.title, state(c.status, added, removed), &c.provider, branch, ago(c.at, now))
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.select_session(id.clone(), window, cx)))
    }

    fn explorer(&mut self, cx: &mut Context<Self>) -> Stateful<Div> {
        let mut rows = Vec::new();
        if let Some(root) = self.project.clone() {
            self.tree(Path::new(&root), 0, &mut rows, cx);
        }
        div().id("explorer").flex_1().mt(px(8.)).p(px(8.)).overflow_y_scroll().flex().flex_col().gap(px(1.)).children(rows)
    }

    fn tree(&self, dir: &Path, depth: usize, rows: &mut Vec<Stateful<Div>>, cx: &mut Context<Self>) {
        for (is_dir, path) in self.tree.get(dir).cloned().unwrap_or_default() {
            let open = is_dir && self.tree.contains_key(&path);
            let target = path.clone();
            let label = path.file_name().unwrap_or_default().to_string_lossy().to_string();
            rows.push(ui::tree_row(id(format!("tree-{}", path.display())), label, is_dir, open, depth).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                if !is_dir {
                    cx.open_with_system(&target);
                } else if this.tree.remove(&target).is_none() {
                    this.tree.insert(target.clone(), list_dir(&target));
                }
                cx.notify();
            })));
            if open {
                self.tree(&path, depth + 1, rows, cx);
            }
        }
    }

    fn main_view(&mut self, cx: &mut Context<Self>) -> Div {
        let body = match self.screen {
            Screen::Inbox => self.inbox_detail(cx),
            Screen::Sessions => match self.session.clone() {
                Some(id) => self.session_view(&id, cx),
                None if self.diff_file.is_some() => self.diff_view(cx),
                None => drag_area(div())
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(14.))
                    .text_color(rgba(TEXT_3))
                    .child(if self.project.is_some() { "Pick a session, or press + to start one." } else { "Add a project to begin." }),
            },
        };
        ui::page(div()).flex_1().min_w_0().h_full().flex().flex_col().overflow_hidden().child(body).child(self.footer())
    }

    pub fn pane_label(&self, id: &str) -> String {
        match (self.summary(id), self.sessions.get(id)) {
            (Some(a), _) => a.provider.clone(),
            (None, Some(s)) => command_line(&s.info),
            (None, None) => "session".into(),
        }
    }

    fn session_view(&mut self, id: &str, cx: &mut Context<Self>) -> Div {
        let (provider, title, model) = match self.summary(id) {
            Some(a) => (a.provider.clone(), a.title.clone(), Some(agents::model_label(a))),
            None => (String::new(), self.pane_label(id), None),
        };
        let branch = self.repo().map(|r| r.branch.clone()).unwrap_or_default();
        let (added, removed) = self.repo().map(|r| r.totals()).unwrap_or_default();
        let header = drag_area(div())
            .h(px(52.))
            .pl(px(20.))
            .pr(px(12.))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(8.))
            .child(dot(8., provider_color(&provider)))
            .child(div().truncate().text_size(px(17.)).font_weight(FontWeight::BOLD).child(title))
            .children(model.map(ui::tag))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .items_center()
                    .gap(px(5.))
                    .pl(px(4.))
                    .child(icon("worktree", 12., TEXT_4))
                    .child(div().truncate().font_family(MONO).text_size(px(11.5)).text_color(rgba(TEXT_3)).child(branch)),
            )
            .child(ui::icon_group([
                ui::group_button("split-right", "split-right").on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.new_shell(Some(false), cx))),
                ui::group_button("split-down", "split-down").on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.new_shell(Some(true), cx))),
            ]))
            .child(
                ui::button("diffstat", Variant::Secondary, Some("branch"), ui::diffstat(added, removed).text_size(px(12.)))
                    .ml(px(4.))
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.open_changes(None, cx))),
            );
        let body = match self.workspace(id).active() {
            Some(Tab::Changes) => self.diff_view(cx),
            Some(Tab::Term(rows)) => {
                let rows = rows.clone();
                self.panes(rows, cx)
            }
            None => div().flex_1(),
        };
        div().flex_1().min_h_0().flex().flex_col().child(header).child(self.tab_strip(id, cx)).child(body)
    }

    fn tab_lead(&self, panes: Option<Vec<String>>, (added, removed): (usize, usize)) -> Div {
        let row = div().flex().items_center().gap(px(7.));
        let Some(p) = panes else {
            return row.child(icon("branch", 13., TEXT_3)).child(format!("Changes · +{added} −{removed}"));
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

    fn tab_strip(&mut self, parent: &str, cx: &mut Context<Self>) -> Div {
        let totals = self.repo().map(|r| r.totals()).unwrap_or_default();
        let w = self.workspace(parent);
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
                    .h(px(30.))
                    .pl(px(12.))
                    .pr(px(4.))
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(8.))
                    .rounded(px(9.))
                    .cursor_pointer()
                    .font_family(MONO)
                    .text_size(px(12.))
                    .whitespace_nowrap()
                    .when(selected, |d| d.bg(rgba(FILL_3)).font_weight(FontWeight::SEMIBOLD).text_color(rgba(TEXT)))
                    .when(!selected, |d| d.font_weight(FontWeight::MEDIUM).text_color(rgba(TEXT_2)).hover(|s| s.bg(rgba(FILL_2))))
                    .child(self.tab_lead(panes, totals))
                    .child(
                        div()
                            .id(("close-tab", i))
                            .size(px(20.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(6.))
                            .hover(|s| s.bg(rgba(FILL_3)))
                            .child(icon("x", 11., TEXT_3))
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                cx.stop_propagation();
                                this.close_tab(i, cx);
                            })),
                    )
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.select_tab(i, window, cx)))
            })
            .collect();
        div()
            .h(px(42.))
            .flex_none()
            .px(px(12.))
            .flex()
            .items_center()
            .gap(px(2.))
            .border_b_1()
            .border_color(rgba(HAIRLINE))
            .children(items)
            .child(icon_button("new-tab", "plus").on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.new_shell(None, cx))))
            .child(div().flex_1())
            .child(div().text_size(px(12.)).text_color(rgba(TEXT_3)).child("tabs of this session"))
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
            out.push(div().flex().gap(px(8.)).min_h_0().when(r == 0, |d| d.flex_1()).when(r > 0, |d| d.h(px(250.)).flex_none()).children(panes));
        }
        div()
            .flex_1()
            .min_h_0()
            .p(px(10.))
            .flex()
            .flex_col()
            .gap(px(8.))
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
                .h(px(28.))
                .flex_none()
                .pl(px(12.))
                .pr(px(4.))
                .flex()
                .items_center()
                .border_b_1()
                .border_color(rgba(HAIRLINE))
                .bg(rgba(if focused { FILL_2 } else { SURFACE_SUNKEN }))
                .text_color(rgba(if focused { TEXT } else { TEXT_3 }))
                .font_family(MONO)
                .text_size(px(11.5))
                .child(div().flex_1().truncate().child(format!("{n} · {title}")))
                .child(
                    div()
                        .id(self::id(format!("close-{close_id}")))
                        .size(px(22.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(6.))
                        .hover(|s| s.bg(rgba(FILL_3)))
                        .child(icon("x", 12., TEXT_3))
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            cx.stop_propagation();
                            this.close_pane(&close_id, cx);
                        })),
                )
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
            .bg(rgba(SURFACE))
            .rounded(px(12.))
            .shadow(vec![ui::ring(if focused || n.is_none() { SEPARATOR_STRONG } else { HAIRLINE }, 1.)])
            .overflow_hidden()
            .on_mouse_down(MouseButton::Left, cx.listener(move |this, _: &MouseDownEvent, window, cx| this.focus_pane(focus_id.clone(), window, cx)))
            .children(header)
            .child(div().flex_1().min_h_0().px(px(16.)).py(px(12.)).font_family(MONO).text_size(px(m.size)).line_height(px(m.line)).text_color(rgba(TEXT)).child(screen))
            .children(exit.map(|c| {
                div().px(px(16.)).pb(px(8.)).text_size(px(12.)).text_color(rgba(if c == 0 { TEXT_3 } else { FAILED })).child(format!("Process exited with code {c}"))
            }))
    }

    fn footer(&self) -> Div {
        let cards = self.project.as_deref().map(|p| self.cards(p)).unwrap_or_default();
        let now = now_ms();
        let providers = ["claude", "codex"].into_iter().filter_map(|p| {
            let mine: Vec<&Card> = cards.iter().filter(|c| c.provider == p).collect();
            if mine.is_empty() {
                return None;
            }
            let agents: Vec<_> = mine.iter().filter_map(|c| self.agents.get(&c.id)).collect();
            let left = agents.iter().max_by_key(|a| a.updated_at).and_then(|a| self.agents.context_left(&a.id));
            let started = agents.iter().map(|a| a.created_at).min().unwrap_or(now);
            let label = match left {
                Some(pct) => format!("{pct}% · {}", elapsed(now - started)),
                None => elapsed(now - started),
            };
            let fill = 44. * left.unwrap_or(0) as f32 / 100.;
            Some(
                div()
                    .flex()
                    .items_center()
                    .gap(px(7.))
                    .child(ui::agent_badge(p, if p == "codex" { "Codex" } else { "Claude" }).text_color(rgba(TEXT)))
                    .child(div().w(px(44.)).h(px(5.)).rounded(px(3.)).bg(rgba(FILL_4)).child(div().h_full().w(px(fill)).rounded(px(3.)).bg(rgba(provider_color(p)))))
                    .child(div().font_family(MONO).text_size(px(11.5)).child(label)),
            )
        });
        let term = self.focused.as_deref().filter(|_| self.screen == Screen::Inbox || self.session.is_some());
        let place = term.map(|id| {
            let project = self.project.as_deref().map(basename).unwrap_or_default();
            let tab = self.session.as_ref().and_then(|s| self.workspaces.get(s)).map_or(0, |w| w.active) + 1;
            format!("{} {project}:{tab}", self.pane_label(id))
        });
        let size = term.and_then(|id| self.sized.get(id)).map(|(c, r)| format!("{c}×{r}"));
        div()
            .h(px(36.))
            .flex_none()
            .pl(px(12.))
            .pr(px(16.))
            .flex()
            .items_center()
            .gap(px(16.))
            .border_t_1()
            .border_color(rgba(HAIRLINE))
            .text_size(px(12.))
            .text_color(rgba(TEXT_2))
            .children(providers)
            .children(self.error.clone().map(|e| div().truncate().text_color(rgba(FAILED)).child(e)))
            .child(div().flex_1())
            .children(place.map(|p| div().flex().items_center().gap(px(6.)).font_family(MONO).text_size(px(11.5)).child(icon("terminal", 12., TEXT_3)).child(p)))
            .children(size.map(|s| div().font_family(MONO).text_size(px(11.5)).child(s)))
    }
}

impl Render for Desktop {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut root = div()
            .size_full()
            .p(px(8.))
            .flex()
            .gap(px(8.))
            .bg(rgba(WINDOW))
            .font_family(SANS)
            .line_height(relative(1.2))
            .text_color(rgba(TEXT))
            .track_focus(&self.root)
            .on_action(cx.listener(Self::focus_search))
            .on_action(cx.listener(Self::open_selected))
            .child(self.rail(cx));
        if self.sidebar {
            root = root.child(self.sidebar_view(cx));
        }
        root.child(self.main_view(cx))
    }
}

#[cfg(test)]
mod tests {
    use super::{Info, ago, command_line, elapsed};

    #[test]
    fn formats_times_like_the_design() {
        let min = 60_000;
        assert_eq!(ago(0, min), "");
        assert_eq!(ago(1, 30_000), "now");
        assert_eq!(ago(1, 1 + 22 * min), "22m");
        assert_eq!(ago(1, 1 + 3 * 60 * min + 5 * min), "3h");
        assert_eq!(ago(1, 1 + 30 * 60 * min), "yesterday");
        assert_eq!(ago(1, 1 + 72 * 60 * min), "3d");
    }

    #[test]
    fn elapsed_reads_like_the_footer() {
        assert_eq!(elapsed(12 * 60_000), "12m");
        assert_eq!(elapsed(108 * 60_000), "1h 48m");
    }

    #[test]
    fn command_line_hides_the_login_flag() {
        let info = Info { cmd: "/bin/zsh".into(), args: vec!["-l".into()], ..Default::default() };
        assert_eq!(command_line(&info), "zsh");
        let info = Info { cmd: "pnpm".into(), args: vec!["dev".into(), "--port".into(), "8081".into()], ..Default::default() };
        assert_eq!(command_line(&info), "pnpm dev --port 8081");
    }
}
