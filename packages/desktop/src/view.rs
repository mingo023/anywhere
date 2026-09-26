use crate::daemon::Info;
use crate::termview::{self, Metrics};
use crate::theme::*;
use crate::workspace::Tab;
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

pub fn shadow(y: f32, blur: f32, alpha: f32) -> BoxShadow {
    BoxShadow { color: hsla(0., 0., 0., alpha), offset: point(px(0.), px(y)), blur_radius: px(blur), spread_radius: px(0.), inset: false }
}

pub fn icon_button(id: impl Into<ElementId>, name: &str, box_size: f32, icon_size: f32) -> Stateful<Div> {
    div()
        .id(id)
        .size(px(box_size))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(7.))
        .hover(|s| s.bg(rgb(HOVER)))
        .child(icon(name, icon_size, MUTED))
}

pub fn kbd(label: &str) -> Div {
    div()
        .h(px(18.))
        .px(px(5.))
        .flex()
        .flex_none()
        .items_center()
        .rounded(px(4.))
        .border_1()
        .border_color(rgb(0xdad8d2))
        .font_family(MONO)
        .text_size(px(11.))
        .text_color(rgb(MUTED))
        .child(label.to_string())
}

pub fn dot(size: f32, color: u32) -> Div {
    div().size(px(size)).flex_none().rounded(px(size / 2.)).bg(rgb(color))
}

pub fn square(size: f32, color: u32) -> Div {
    div().size(px(size)).flex_none().rounded(px(size * 0.3)).bg(rgb(color))
}

pub fn button(id: impl Into<ElementId>) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(30.))
        .px(px(12.))
        .flex()
        .flex_none()
        .items_center()
        .gap(px(6.))
        .rounded(px(8.))
        .border_1()
        .border_color(rgb(FIELD))
        .bg(rgb(WHITE))
        .text_size(px(13.))
        .text_color(rgb(INK))
        .hover(|s| s.bg(rgb(0xfbfaf8)))
}

pub fn segmented<T: Copy + PartialEq + 'static>(
    items: Vec<(T, String)>,
    active: T,
    height: f32,
    size: f32,
    on: impl Fn(&mut Desktop, T, &mut Context<Desktop>) + 'static,
    cx: &mut Context<Desktop>,
) -> Div {
    let on = std::rc::Rc::new(on);
    div().p(px(3.)).flex().gap(px(2.)).rounded(px(9.)).bg(rgb(SEGMENT)).children(items.into_iter().enumerate().map(|(i, (v, label))| {
        let on = on.clone();
        let selected = v == active;
        div()
            .id(("segment", i))
            .flex_1()
            .h(px(height))
            .px(px(10.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(7.))
            .text_size(px(size))
            .whitespace_nowrap()
            .when(selected, |d| d.bg(rgb(WHITE)).font_weight(FontWeight::SEMIBOLD).text_color(rgb(INK)).shadow(vec![shadow(1., 2., 0.08)]))
            .when(!selected, |d| d.text_color(rgb(MUTED)).font_weight(FontWeight::MEDIUM))
            .child(label)
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| on(this, v, cx)))
    }))
}

fn status_badge(status: Status, i: usize) -> Div {
    let row = div().flex().flex_none().items_center().gap(px(5.)).text_size(px(12.5));
    match status {
        Status::NeedsYou => row.child(dot(7., AMBER)).font_weight(FontWeight::SEMIBOLD).text_color(rgb(AMBER_TEXT)).child("Needs you"),
        Status::Working => row.child(spinner(("working", i), 12., GREEN)).font_weight(FontWeight::MEDIUM).text_color(rgb(GREEN)).child("Working"),
        Status::Done => row.child(icon("check", 12., MUTED)).text_color(rgb(MUTED)).child("Done"),
        Status::Failed => row.child(icon("x", 12., RED)).font_weight(FontWeight::SEMIBOLD).text_color(rgb(RED)).child("Failed"),
    }
}

fn rail_row(id: &'static str, name: &str, label: &str, selected: bool) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(34.))
        .px(px(10.))
        .flex()
        .flex_none()
        .items_center()
        .gap(px(10.))
        .rounded(px(8.))
        .text_size(px(14.))
        .text_color(rgb(TEXT))
        .when(selected, |d| d.bg(rgb(PROJECT_SELECTED)).font_weight(FontWeight::SEMIBOLD).text_color(rgb(INK)))
        .when(!selected, |d| d.font_weight(FontWeight::MEDIUM).hover(|s| s.bg(rgb(HOVER))))
        .child(icon(name, 16., MUTED))
        .child(div().flex_1().child(label.to_string()))
}

pub fn sidebar_frame() -> Div {
    div().w(px(332.)).flex_none().h_full().flex().flex_col().bg(rgb(APP)).border_r_1().border_color(rgb(LINE))
}

impl Desktop {
    fn rail(&self, cx: &mut Context<Self>) -> Div {
        let top = drag_area(div())
            .h(px(52.))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(1.))
            .child(div().flex_1())
            .child(icon_button("back", "back", 28., 16.))
            .child(icon_button("forward", "forward", 28., 16.))
            .child(icon_button("toggle-sidebar", "sidebar", 28., 16.).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.sidebar = !this.sidebar;
                cx.notify();
            })));
        let search = div()
            .id("rail-search")
            .h(px(34.))
            .px(px(10.))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(8.))
            .rounded(px(8.))
            .border_1()
            .border_color(rgb(FIELD))
            .bg(rgb(WHITE))
            .text_size(px(14.))
            .text_color(rgb(MUTED))
            .child(icon("search", 15., MUTED))
            .child(div().flex_1().child("Search"))
            .child(kbd("⌘K"))
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.focus_search(&crate::FocusSearch, window, cx)));
        let asks = self.agents.pending.len();
        let inbox = self.screen == Screen::Inbox;
        let badge = div()
            .px(px(6.))
            .min_w(px(18.))
            .flex()
            .justify_center()
            .rounded(px(9.))
            .bg(rgb(AMBER_TEXT))
            .font_family(MONO)
            .text_size(px(11.5))
            .font_weight(FontWeight::SEMIBOLD)
            .line_height(px(18.))
            .text_color(rgb(WHITE))
            .child(asks.to_string());
        let nav = div()
            .mt(px(12.))
            .flex()
            .flex_col()
            .gap(px(2.))
            .child(rail_row("inbox", "inbox", "Inbox", inbox).when(asks > 0, |d| d.child(badge)).on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                this.open_inbox(window, cx);
            })))
            .child(rail_row("automations", "bolt", "Automations", false));
        let header = div()
            .pt(px(20.))
            .pb(px(6.))
            .px(px(10.))
            .flex()
            .items_center()
            .text_size(px(12.5))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(rgb(MUTED))
            .child(div().flex_1().child("Projects"))
            .child(icon_button("add-project", "plus", 22., 14.).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.add_project(cx))));
        let projects = self.projects().into_iter().enumerate().map(|(i, p)| {
            let selected = self.screen == Screen::Sessions && self.project.as_ref() == Some(&p);
            let cards = self.cards(&p);
            let needs = cards.iter().any(|c| c.status == Status::NeedsYou);
            let running = cards.iter().filter(|c| c.status == Status::Working).count();
            let name = basename(&p);
            div()
                .id(("project", i))
                .h(px(34.))
                .px(px(10.))
                .flex()
                .flex_none()
                .items_center()
                .gap(px(10.))
                .rounded(px(8.))
                .text_size(px(14.))
                .text_color(rgb(TEXT))
                .when(selected, |d| d.bg(rgb(PROJECT_SELECTED)).font_weight(FontWeight::SEMIBOLD).text_color(rgb(INK)))
                .when(!selected, |d| d.font_weight(FontWeight::MEDIUM).hover(|s| s.bg(rgb(HOVER))))
                .child(square(10., project_color(i)))
                .child(div().flex_1().truncate().child(name))
                .when(needs, |d| {
                    d.child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(5.))
                            .text_size(px(11.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(AMBER_TEXT))
                            .child(dot(7., AMBER))
                            .child("needs you"),
                    )
                })
                .when(!needs && running > 0, |d| {
                    d.child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(5.))
                            .font_family(MONO)
                            .text_size(px(11.5))
                            .text_color(rgb(GREEN))
                            .child(spinner(("project-spin", i), 12., GREEN))
                            .child(running.to_string()),
                    )
                })
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.select_project(p.clone(), cx)))
        });
        div()
            .w(px(248.))
            .flex_none()
            .h_full()
            .px(px(10.))
            .pb(px(12.))
            .flex()
            .flex_col()
            .bg(rgb(RAIL))
            .border_r_1()
            .border_color(rgb(LINE))
            .child(top)
            .child(search)
            .child(nav)
            .child(header)
            .child(div().id("projects").flex_1().overflow_y_scroll().flex().flex_col().gap(px(2.)).children(projects))
            .child(rail_row("settings", "settings", "Settings", false).child(kbd("⌘,")))
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
            .child(div().flex_1().truncate().text_size(px(16.)).font_weight(FontWeight::BOLD).child(name))
            .child(icon_button("find", "search", 28., 16.).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.focus_search(&crate::FocusSearch, window, cx))))
            .child(icon_button("new-session", "plus", 28., 16.).on_click(cx.listener(|this, e: &ClickEvent, _, cx| this.new_session(if e.modifiers().alt { "codex" } else { "claude" }, cx))));
        let tabs = segmented(
            vec![(Side::Sessions, "Sessions".into()), (Side::Explorer, "Explorer".into()), (Side::Changes, format!("Changes · {changes}"))],
            self.side,
            30.,
            13.5,
            |this, side, cx| {
                this.side = side;
                if side == Side::Changes {
                    this.refresh_git(cx);
                }
                cx.notify();
            },
            cx,
        );
        sidebar_frame().child(header).child(div().px(px(12.)).child(tabs)).child(body)
    }

    fn session_list(&mut self, cx: &mut Context<Self>) -> Div {
        let search = div()
            .mx(px(12.))
            .mt(px(12.))
            .h(px(38.))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(8.))
            .border_t_1()
            .border_b_1()
            .border_color(rgb(HAIR))
            .pl(px(6.))
            .child(icon("search", 15., MUTED))
            .child(div().flex_1().child(Input::new(&self.filter).appearance(false).p_0().text_size(px(13.5))))
            .child(icon_button("filter", "filter", 26., 14.));
        let list = div().flex_1().min_h_0().flex().flex_col().child(search);
        let Some(project) = self.project.clone() else {
            return list.child(div().p(px(16.)).text_size(px(13.5)).text_color(rgb(MUTED)).child("Add a project with + to start."));
        };
        let query = self.filter.read(cx).value().to_lowercase();
        let now = now_ms();
        let cards = self.cards(&project).into_iter().filter(|c| query.is_empty() || c.title.to_lowercase().contains(&query));
        let cards: Vec<_> = cards.enumerate().map(|(i, c)| self.card(i, c, now, cx)).collect();
        list.child(div().id("cards").flex_1().overflow_y_scroll().px(px(8.)).py(px(8.)).flex().flex_col().gap(px(2.)).children(cards))
    }

    fn card(&self, i: usize, c: Card, now: i64, cx: &mut Context<Self>) -> Stateful<Div> {
        let selected = self.session.as_ref() == Some(&c.id);
        let branch = self.repos.get(&c.cwd).map(|r| r.branch.clone()).unwrap_or_default();
        let id = c.id.clone();
        div()
            .id(("card", i))
            .px(px(12.))
            .py(px(10.))
            .flex()
            .flex_none()
            .flex_col()
            .gap(px(4.))
            .rounded(px(10.))
            .when(selected, |d| d.bg(rgb(SELECTED)))
            .when(!selected, |d| d.hover(|s| s.bg(rgb(0xefede9))))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .text_size(px(12.5))
                    .child(square(7., provider_color(&c.provider)))
                    .child(div().font_weight(FontWeight::SEMIBOLD).text_color(rgb(provider_color(&c.provider))).child(provider_name(&c.provider)))
                    .child(div().flex_1().truncate().text_color(rgb(MUTED)).child(c.model))
                    .child(div().text_color(rgb(MUTED)).child(ago(c.at, now))),
            )
            .child(div().truncate().text_size(px(14.5)).font_weight(FontWeight::SEMIBOLD).text_color(rgb(INK)).child(c.title))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(icon("branch", 12., MUTED))
                    .child(div().flex_1().truncate().font_family(MONO).text_size(px(12.)).text_color(rgb(MUTED)).child(branch))
                    .child(status_badge(c.status, i)),
            )
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.select_session(id.clone(), window, cx)))
    }

    fn explorer(&mut self, cx: &mut Context<Self>) -> Stateful<Div> {
        let mut rows = Vec::new();
        if let Some(root) = self.project.clone() {
            self.tree(Path::new(&root), 0, &mut rows, cx);
        }
        div().id("explorer").flex_1().mt(px(10.)).px(px(8.)).pb(px(8.)).overflow_y_scroll().flex().flex_col().children(rows)
    }

    fn tree(&self, dir: &Path, depth: usize, rows: &mut Vec<Stateful<Div>>, cx: &mut Context<Self>) {
        for (is_dir, path) in self.tree.get(dir).cloned().unwrap_or_default() {
            let open = is_dir && self.tree.contains_key(&path);
            let target = path.clone();
            rows.push(
                div()
                    .id(id(format!("tree-{}", path.display())))
                    .h(px(30.))
                    .pl(px(10. + depth as f32 * 14.))
                    .pr(px(10.))
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(8.))
                    .rounded(px(7.))
                    .text_size(px(13.5))
                    .text_color(rgb(TEXT))
                    .hover(|s| s.bg(rgb(HOVER)))
                    .child(icon(if is_dir { "folder" } else { "file" }, 14., MUTED))
                    .child(div().flex_1().truncate().child(path.file_name().unwrap_or_default().to_string_lossy().to_string()))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        if !is_dir {
                            cx.open_with_system(&target);
                        } else if this.tree.remove(&target).is_none() {
                            this.tree.insert(target.clone(), list_dir(&target));
                        }
                        cx.notify();
                    })),
            );
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
                    .text_color(rgb(MUTED))
                    .child(if self.project.is_some() { "Pick a session, or press + to start one." } else { "Add a project to begin." }),
            },
        };
        div().flex_1().min_w_0().h_full().flex().flex_col().bg(rgb(MAIN)).child(body).child(self.footer())
    }

    pub fn pane_label(&self, id: &str) -> String {
        match (self.summary(id), self.sessions.get(id)) {
            (Some(a), _) => a.provider.clone(),
            (None, Some(s)) => command_line(&s.info),
            (None, None) => "session".into(),
        }
    }

    fn session_view(&mut self, id: &str, cx: &mut Context<Self>) -> Div {
        let (provider, title) = match self.summary(id) {
            Some(a) => (a.provider.clone(), a.title.clone()),
            None => (String::new(), self.pane_label(id)),
        };
        let branch = self.repo().map(|r| r.branch.clone()).unwrap_or_default();
        let (added, removed) = self.repo().map(|r| r.totals()).unwrap_or_default();
        let header = drag_area(div())
            .h(px(46.))
            .pl(px(20.))
            .pr(px(12.))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(8.))
            .child(dot(8., provider_color(&provider)))
            .child(div().truncate().text_size(px(15.)).font_weight(FontWeight(650.)).child(title))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .items_center()
                    .gap(px(5.))
                    .pl(px(4.))
                    .child(icon("branch", 12., MUTED))
                    .child(div().truncate().font_family(MONO).text_size(px(12.)).text_color(rgb(MUTED)).child(branch)),
            )
            .child(icon_button("split-right", "split-right", 30., 16.).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.new_shell(Some(false), cx))))
            .child(icon_button("split-down", "split-down", 30., 16.).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.new_shell(Some(true), cx))))
            .child(
                button("diffstat")
                    .ml(px(4.))
                    .px(px(10.))
                    .font_family(MONO)
                    .text_size(px(12.5))
                    .child(icon("branch", 12., MUTED))
                    .child(div().text_color(rgb(GREEN)).child(format!("+{added}")))
                    .child(div().text_color(rgb(RED)).child(format!("−{removed}")))
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
            return row.child(icon("branch", 13., MUTED)).child(format!("Changes · +{added} −{removed}"));
        };
        let count = |label: String| if p.len() > 1 { format!("{label} · {} panes", p.len()) } else { label };
        if let Some(a) = self.summary(&p[0]) {
            return row
                .child(dot(7., provider_color(&a.provider)))
                .child(count(a.provider.clone()))
                .when(self.agents.needs_you(&p[0]), |d| d.child(dot(6., AMBER)));
        }
        let mark = match self.sessions.get(&p[0]).map(|s| s.exit) {
            Some(None) => dot(6., LIVE).into_any_element(),
            Some(Some(c)) if c != 0 => icon("x", 12., RED).into_any_element(),
            _ => dot(6., FAINT).into_any_element(),
        };
        row.child(icon("terminal", 13., MUTED)).child(count(self.pane_label(&p[0]))).child(mark)
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
                    .h(px(32.))
                    .pl(px(12.))
                    .pr(px(6.))
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(8.))
                    .rounded_t(px(8.))
                    .font_family(MONO)
                    .text_size(px(12.5))
                    .whitespace_nowrap()
                    .when(selected, |d| {
                        d.bg(rgb(WHITE))
                            .border_1()
                            .border_b_0()
                            .border_color(rgb(TAB_LINE))
                            .mb(px(-1.))
                            .h(px(33.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(INK))
                    })
                    .when(!selected, |d| d.font_weight(FontWeight::MEDIUM).text_color(rgb(MUTED)).hover(|s| s.bg(rgb(0xf0eeea))))
                    .child(self.tab_lead(panes, totals))
                    .child(icon_button(("close-tab", i), "x", 18., 11.).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        this.close_tab(i, cx);
                    })))
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.select_tab(i, window, cx)))
            })
            .collect();
        div()
            .h(px(38.))
            .flex_none()
            .px(px(12.))
            .flex()
            .items_end()
            .gap(px(2.))
            .border_b_1()
            .border_color(rgb(TAB_LINE))
            .children(items)
            .child(
                div()
                    .h(px(32.))
                    .flex()
                    .items_center()
                    .child(icon_button("new-tab", "plus", 28., 15.).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.new_shell(None, cx)))),
            )
            .child(div().flex_1())
            .child(div().h(px(32.)).flex().items_center().text_size(px(12.)).text_color(rgb(MUTED)).child("tabs of this session"))
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
            .key_context(crate::keys::CONTEXT)
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
            None => div().text_color(rgb(MUTED)).child(if known { "Connecting…" } else { "This session is not running." }),
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
                .border_color(rgb(PANE_LINE))
                .bg(rgb(if focused { 0xf4f2ee } else { MAIN }))
                .text_color(rgb(if focused { INK } else { MUTED }))
                .font_family(MONO)
                .text_size(px(11.5))
                .child(div().flex_1().truncate().child(format!("{n} · {title}")))
                .child(icon_button(self::id(format!("close-{close_id}")), "x", 22., 12.).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
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
            .bg(rgb(WHITE))
            .rounded(px(8.))
            .border_1()
            .border_color(rgb(if focused || n.is_none() { TERM_LINE } else { CARD }))
            .overflow_hidden()
            .on_mouse_down(MouseButton::Left, cx.listener(move |this, _: &MouseDownEvent, window, cx| this.focus_pane(focus_id.clone(), window, cx)))
            .children(header)
            .child(
                div().flex_1().min_h_0().px(px(16.)).py(px(12.)).font_family(MONO).text_size(px(m.size)).line_height(px(m.line)).text_color(rgb(TERM_TEXT)).child(screen),
            )
            .children(exit.map(|c| {
                div().px(px(16.)).pb(px(8.)).text_size(px(12.)).text_color(rgb(if c == 0 { MUTED } else { RED })).child(format!("Process exited with code {c}"))
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
                    .child(dot(7., provider_color(p)))
                    .child(if p == "codex" { "Codex" } else { "Claude" })
                    .child(div().w(px(44.)).h(px(5.)).rounded(px(3.)).bg(rgb(TAB_LINE)).child(div().h_full().w(px(fill)).rounded(px(3.)).bg(rgb(provider_color(p)))))
                    .child(div().font_family(MONO).text_size(px(12.)).child(label)),
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
            .h(px(32.))
            .flex_none()
            .pl(px(16.))
            .pr(px(12.))
            .flex()
            .items_center()
            .gap(px(16.))
            .border_t_1()
            .border_color(rgb(HAIR))
            .text_size(px(12.5))
            .text_color(rgb(MUTED))
            .children(providers)
            .children(self.error.clone().map(|e| div().truncate().text_color(rgb(RED)).child(e)))
            .child(div().flex_1())
            .children(place.map(|p| div().flex().items_center().gap(px(6.)).font_family(MONO).text_size(px(12.)).child(icon("terminal", 12., MUTED)).child(p)))
            .children(size.map(|s| div().font_family(MONO).text_size(px(12.)).child(s)))
    }
}

impl Render for Desktop {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut root = div()
            .size_full()
            .flex()
            .bg(rgb(APP))
            .font_family(SANS)
            .text_color(rgb(INK))
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
