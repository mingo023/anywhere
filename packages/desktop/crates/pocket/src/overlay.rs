use crate::view::{ago, basename, now_ms, tilde};
use crate::{Card, Desktop, Overlay, Screen, Side, Status};
use gpui_kit::component::input::Input;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::cmp::Reverse;
use std::path::Path;
use theme::*;
use ui::{self, dot, menu_row};

#[derive(Clone)]
pub enum Pick {
    Session(String),
    File(String),
    New,
    Split,
    Next,
}

#[derive(Clone)]
enum Lead {
    Waiting,
    Running,
    Provider(String),
    Icon(&'static str),
}

#[derive(Clone)]
pub struct Entry {
    pub pick: Pick,
    lead: Lead,
    title: String,
    detail: String,
    keys: Option<&'static str>,
}

fn status_word(s: Status) -> &'static str {
    match s {
        Status::NeedsYou => "needs you",
        Status::Failed => "failed",
        Status::Done => "done",
        Status::Working => "working",
        Status::Idle => "idle",
    }
}

fn hint(keys: &str, label: &str) -> Div {
    div().flex().items_center().gap(px(4.)).child(div().text_color(rgba(TEXT_3)).child(keys.to_string())).child(label.to_string())
}

impl Desktop {
    pub fn open_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.palette_ix = 0;
        self.filter.update(cx, |s, cx| {
            s.set_value("", window, cx);
            s.focus(window, cx);
        });
        let Some(root) = self.explore_root() else { return };
        let task = cx.background_executor().spawn(async move { git::ls_files(&root) });
        cx.spawn(async move |this, cx| {
            let files = task.await;
            this.update(cx, |d, cx| {
                d.palette_files = files;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub fn palette_sections(&self, cx: &App) -> Vec<(&'static str, Vec<Entry>)> {
        let q = self.filter.read(cx).value().to_lowercase();
        let hit = |s: &str| q.is_empty() || s.to_lowercase().contains(&q);
        let projects = if self.palette_all { self.projects() } else { self.project.iter().cloned().collect() };
        let mut cards: Vec<(String, Card)> = projects
            .iter()
            .flat_map(|p| {
                let name = self.repo_name(p);
                self.cards(p).into_iter().map(move |c| (name.clone(), c))
            })
            .filter(|(_, c)| hit(&c.title))
            .collect();
        cards.sort_by_key(|(_, c)| (c.status, Reverse(c.at)));
        let sessions: Vec<Entry> = cards
            .into_iter()
            .take(5)
            .map(|(name, c)| Entry {
                lead: match c.status {
                    Status::NeedsYou => Lead::Waiting,
                    Status::Working => Lead::Running,
                    _ => Lead::Provider(c.provider.clone()),
                },
                detail: format!("{name} · {} · {}", provider_name(&c.provider), status_word(c.status)),
                title: c.title,
                keys: None,
                pick: Pick::Session(c.id),
            })
            .collect();
        let root = self.explore_root().unwrap_or_default();
        let changed: Vec<String> = self.repos.get(&root).map(|r| r.files.iter().map(|f| f.path.clone()).collect()).unwrap_or_default();
        let mut paths: Vec<&String> = if q.is_empty() { changed.iter().take(3).collect() } else { self.palette_files.iter().filter(|p| hit(p)).collect() };
        paths.sort_by_key(|p| !changed.contains(p));
        let files = paths
            .into_iter()
            .take(5)
            .map(|p| {
                let dir = p.rsplit_once('/').map(|(d, _)| d.to_string()).unwrap_or_default();
                let detail = match (dir.is_empty(), changed.contains(p)) {
                    (true, true) => "modified".to_string(),
                    (false, true) => format!("{dir} · modified"),
                    _ => dir,
                };
                Entry { pick: Pick::File(format!("{root}/{p}")), lead: Lead::Icon("file"), title: basename(p), detail, keys: None }
            })
            .collect();
        let project = self.project.as_deref().map(|p| self.repo_name(p)).unwrap_or_default();
        let actions = [
            Entry { pick: Pick::New, lead: Lead::Icon("sparkle"), title: format!("New session in {project}"), detail: String::new(), keys: Some("⌘ N") },
            Entry { pick: Pick::Split, lead: Lead::Icon("split-right"), title: "Open selected in a split".into(), detail: String::new(), keys: Some("⌘ ↵") },
            Entry { pick: Pick::Next, lead: Lead::Waiting, title: "Jump to next waiting session".into(), detail: String::new(), keys: Some("⌘ J") },
        ]
        .into_iter()
        .filter(|e| hit(&e.title))
        .collect();
        [("Sessions", sessions), ("Files", files), ("Actions", actions)].into_iter().filter(|(_, e)| !e.is_empty()).collect()
    }

    fn activate(&mut self, pick: Pick, split: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.overlay = None;
        match pick {
            Pick::Session(id) => match self.session.clone().filter(|s| split && *s != id) {
                Some(current) => {
                    self.workspace(&current).split(id.clone(), false);
                    self.focus_pane(id, window, cx);
                }
                None => {
                    self.side = Side::Sessions;
                    self.select_session(id, window, cx);
                }
            },
            Pick::File(path) => {
                self.screen = Screen::Sessions;
                self.open_file(path, cx);
            }
            Pick::New => self.open(Overlay::NewSession, window, cx),
            Pick::Split => self.new_shell(Some(false), cx),
            Pick::Next => self.next_waiting(&crate::NextWaiting, window, cx),
        }
        cx.notify();
    }

    fn on_palette_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let entries: Vec<Entry> = self.palette_sections(cx).into_iter().flat_map(|(_, e)| e).collect();
        let last = entries.len().saturating_sub(1);
        match ev.keystroke.key.as_str() {
            "up" => self.palette_ix = self.palette_ix.saturating_sub(1),
            "down" => self.palette_ix = (self.palette_ix + 1).min(last),
            "tab" => {
                self.palette_all = !self.palette_all;
                self.palette_ix = 0;
            }
            "escape" => self.close_overlay(window, cx),
            "enter" => {
                if let Some(e) = entries.get(self.palette_ix.min(last)) {
                    self.activate(e.pick.clone(), ev.keystroke.modifiers.platform, window, cx);
                }
            }
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn palette(&mut self, cx: &mut Context<Self>) -> Div {
        let sections = self.palette_sections(cx);
        let total: usize = sections.iter().map(|(_, e)| e.len()).sum();
        let selected = self.palette_ix.min(total.saturating_sub(1));
        let mut n = 0;
        let mut body = div().id("palette-results").max_h(px(460.)).overflow_y_scroll().px(px(8.)).pb(px(8.)).flex().flex_col();
        for (label, entries) in sections {
            body = body.child(div().pt(px(12.)).pb(px(6.)).px(px(12.)).text_size(px(11.5)).font_weight(FontWeight::SEMIBOLD).text_color(rgba(TEXT_4)).child(label));
            for e in entries {
                let i = n;
                n += 1;
                let lead = match &e.lead {
                    Lead::Waiting => dot(8., WAITING).into_any_element(),
                    Lead::Running => spinner(("palette-spin", i), 12., RUNNING_TEXT).into_any_element(),
                    Lead::Provider(p) => dot(8., provider_color(p)).into_any_element(),
                    Lead::Icon(name) => icon(name, 13., TEXT_2).into_any_element(),
                };
                let keys = e.keys.or((i == selected).then_some("↵"));
                let pick = e.pick.clone();
                body = body.child(ui::palette_row(("palette-row", i), i == selected, lead, e.title, e.detail, keys).on_click(cx.listener(
                    move |this, ev: &ClickEvent, window, cx| this.activate(pick.clone(), ev.modifiers().platform, window, cx),
                )));
            }
        }
        if total == 0 {
            body = body.child(div().p(px(20.)).text_size(px(13.5)).text_color(rgba(TEXT_3)).child("No matches."));
        }
        let input = div()
            .h(px(60.))
            .px(px(20.))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(12.))
            .border_b(px(0.5))
            .border_color(rgba(SEPARATOR))
            .child(icon("search", 17., TEXT_3))
            .child(div().flex_1().text_size(px(17.)).child(Input::new(&self.filter).appearance(false).p_0().text_size(px(17.))))
            .child(div().text_size(px(12.)).text_color(rgba(TEXT_4)).child("esc"));
        let scope = if self.palette_all { "Tab to filter by project" } else { "Tab to search all projects" };
        let footer = div()
            .h(px(40.))
            .px(px(20.))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(16.))
            .border_t(px(0.5))
            .border_color(rgba(SEPARATOR))
            .text_size(px(12.))
            .text_color(rgba(TEXT_4))
            .child(hint("↑↓", "navigate"))
            .child(hint("↵", "open"))
            .child(hint("⌘↵", "open in split"))
            .child(div().ml_auto().child(scope));
        div().absolute().top(px(120.)).left_0().right_0().flex().justify_center().child(
            ui::pop(div().w(px(660.)).rounded(px(22.)).overflow_hidden().flex().flex_col())
                .occlude()
                .capture_key_down(cx.listener(Self::on_palette_key))
                .child(input)
                .child(body)
                .child(footer),
        )
    }

    fn project_menu(&mut self, cx: &mut Context<Self>) -> Div {
        let current = self.project.clone().unwrap_or_default();
        let name = self.repo_name(&current);
        let repos = self.projects().into_iter().enumerate().map(|(i, p)| {
            let selected = p == current;
            let target = p.clone();
            div()
                .id(("menu-repo", i))
                .h(px(30.))
                .px(px(10.))
                .flex()
                .items_center()
                .gap(px(10.))
                .rounded(px(8.))
                .cursor_pointer()
                .when(selected, |d| d.bg(rgba(ACCENT_BG)))
                .when(!selected, |d| d.hover(|s| s.bg(rgba(FILL_2))))
                .child(ui::swatch(self.repo_color(&p), 14., 4.))
                .child(div().text_size(px(14.)).font_weight(FontWeight::MEDIUM).child(self.repo_name(&p)))
                .child(div().flex_1().min_w_0().truncate().font_family(MONO).text_size(px(11.)).text_color(rgba(TEXT_4)).child(tilde(&p)))
                .when(selected, |d| d.child(icon("check", 13., ACCENT)))
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.select_project(target.clone(), cx);
                    this.close_overlay(window, cx);
                }))
        });
        let trees = self.worktrees.get(&current).cloned().unwrap_or_default();
        let merged: Vec<String> = trees.iter().filter(|w| !w.main && self.merged.contains(&w.branch)).map(|w| w.path.clone()).collect();
        let cards = self.cards(&current);
        let rows = trees.iter().enumerate().map(|(i, w)| {
            let card = cards.iter().find(|c| self.worktree_of(&c.cwd).is_some_and(|x| x.path == w.path));
            let repo = self.repos.get(&w.path);
            let is_merged = merged.contains(&w.path);
            let right = if w.main {
                div().text_size(px(11.5)).text_color(rgba(TEXT_4)).child("main checkout").into_any_element()
            } else if is_merged {
                let path = w.path.clone();
                div()
                    .id(("menu-clean", i))
                    .px(px(8.))
                    .py(px(2.))
                    .rounded(px(10.))
                    .cursor_pointer()
                    .bg(rgba(MERGED_BG))
                    .text_size(px(12.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgba(MERGED))
                    .child("Merged · Clean up")
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.remove_worktrees(vec![path.clone()], cx)))
                    .into_any_element()
            } else if let Some(r) = repo.filter(|r| r.ahead + r.behind > 0 || card.is_some_and(|c| matches!(c.status, Status::NeedsYou | Status::Working))) {
                let counts = [(r.ahead > 0).then(|| format!("↑{}", r.ahead)), (r.behind > 0).then(|| format!("↓{}", r.behind))];
                div().flex().gap(px(6.)).font_family(MONO).text_size(px(11.5)).text_color(rgba(TEXT_3)).children(counts.into_iter().flatten()).into_any_element()
            } else {
                let path = w.path.clone();
                ui::button(("menu-remove", i), ui::Variant::Secondary, None, "Remove")
                    .h(px(24.))
                    .px(px(10.))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.remove_worktrees(vec![path.clone()], cx)))
                    .into_any_element()
            };
            let session = card.map(|c| {
                let mark = match c.status {
                    Status::NeedsYou => dot(6., WAITING).into_any_element(),
                    Status::Working => spinner(("menu-spin", i), 11., RUNNING_TEXT).into_any_element(),
                    Status::Failed => icon("x", 11., FAILED).into_any_element(),
                    Status::Done => dot(6., ACCENT).into_any_element(),
                    Status::Idle => div().into_any_element(),
                };
                let title = match c.status {
                    Status::Failed => format!("{} · idle {}", c.title, ago(c.at, now_ms())),
                    _ => c.title.clone(),
                };
                div().flex().items_center().gap(px(5.)).text_size(px(12.5)).text_color(rgba(TEXT_2)).child(mark).child(title)
            });
            let target = w.path.clone();
            div()
                .id(("menu-tree", i))
                .py(px(6.))
                .px(px(10.))
                .flex()
                .items_center()
                .gap(px(12.))
                .rounded(px(10.))
                .cursor_pointer()
                .hover(|s| s.bg(rgba(FILL_2)))
                .child(
                    div()
                        .size(px(28.))
                        .flex()
                        .flex_none()
                        .items_center()
                        .justify_center()
                        .rounded(px(8.))
                        .bg(rgba(FILL_3))
                        .child(icon(if w.main { "folder" } else { "worktree" }, 13., TEXT_3)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(1.))
                        .child(div().truncate().font_family(MONO).text_size(px(12.5)).font_weight(FontWeight::BOLD).child(if w.main { "main".to_string() } else { w.branch.clone() }))
                        .child(div().truncate().font_family(MONO).text_size(px(11.)).text_color(rgba(TEXT_4)).child(tilde(&w.path)))
                        .children(session),
                )
                .child(right)
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.worktree = Some(target.clone());
                    this.session = None;
                    this.close_overlay(window, cx);
                }))
        });
        let clean = (!merged.is_empty()).then(|| {
            let all = merged.clone();
            ui::link("clean-merged", format!("Clean up merged ({})", merged.len()))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.remove_worktrees(all.clone(), cx)))
        });
        let label = |text: String| div().px(px(10.)).pb(px(6.)).text_size(px(12.)).font_weight(FontWeight::SEMIBOLD).text_color(rgba(TEXT_3)).child(text);
        let rule = || div().mx(px(8.)).my(px(8.)).h(px(0.5)).bg(rgba(SEPARATOR));
        let left = if self.wide { 292. } else if self.rail_open { 260. } else { 92. };
        ui::pop(div().absolute().left(px(left)).top(px(62.)).w(px(438.)).p(px(8.)).pt(px(14.)).rounded(px(20.)).flex().flex_col())
            .occlude()
            .child(label(format!("Repositories in {name}")))
            .children(repos)
            .child(rule())
            .child(div().flex().items_center().pr(px(10.)).child(label(format!("Worktrees · {}", trees.len())).flex_1().pb_0()).children(clean))
            .child(div().id("menu-trees").max_h(px(400.)).overflow_y_scroll().pt(px(4.)).flex().flex_col().children(rows))
            .child(rule())
            .child(menu_row("menu-new-worktree", "worktree", "New worktree…", Some("⌘⇧N")).on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                this.new_worktree(&crate::NewWorktree, window, cx)
            })))
            .child(menu_row("menu-add-repo", "plus", "Add repository to project…", None).on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                this.open(Overlay::AddRepo, window, cx)
            })))
            .child(menu_row("menu-settings", "settings", "Project settings", Some("⌘,")).on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                this.project_settings(&crate::ProjectSettings, window, cx)
            })))
    }

    fn more_menu(&mut self, cx: &mut Context<Self>) -> Div {
        let menu = ui::pop(div().absolute().right(px(22.)).top(px(58.)).w(px(230.)).p(px(6.)).rounded(px(16.)).flex().flex_col()).occlude();
        let path = match self.side {
            Side::Explorer => self.file.clone(),
            Side::Changes => self.cwd().zip(self.diff_file.clone()).map(|(cwd, f)| format!("{cwd}/{f}")),
            Side::Sessions => None,
        };
        let Some(path) = path.filter(|_| self.screen == Screen::Sessions) else {
            return menu
                .child(menu_row("more-tab", "terminal", "New terminal tab", None).on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                    this.new_shell(None, cx);
                    this.close_overlay(window, cx);
                })))
                .child(menu_row("more-close", "x", "Close session", None).on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                    if let Some(id) = this.session.take() {
                        this.close_pane(&id, cx);
                    }
                    this.close_overlay(window, cx);
                })));
        };
        let reveal = path.clone();
        menu.child(menu_row("more-open", "external", "Open in editor", None).on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
            cx.open_with_system(Path::new(&path));
            this.close_overlay(window, cx);
        })))
        .child(menu_row("more-reveal", "folder", "Reveal in Finder", None).on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
            cx.reveal_path(Path::new(&reveal));
            this.close_overlay(window, cx);
        })))
    }

    pub fn remove_worktrees(&mut self, paths: Vec<String>, cx: &mut Context<Self>) {
        let Some(repo) = self.project.clone() else { return };
        let task = cx.background_executor().spawn(async move { paths.iter().filter(|p| !git::remove_worktree(&repo, p)).count() });
        cx.spawn(async move |this, cx| {
            let failed = task.await;
            this.update(cx, |d, cx| {
                if failed > 0 {
                    d.error = Some(format!("Couldn't remove {failed} worktree(s); they may have uncommitted changes."));
                }
                d.refresh_git(cx);
            })
            .ok();
        })
        .detach();
    }

    pub fn overlay_view(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let o = self.overlay?;
        let (body, alpha) = match o {
            Overlay::Palette => (self.palette(cx), 0x1f),
            Overlay::NewSession => (self.new_session_view(window, cx), 0x2e),
            Overlay::AddRepo => (self.repo_view(window, cx), 0x40),
            Overlay::ProjectMenu => (self.project_menu(cx), 0),
            Overlay::More => (self.more_menu(cx), 0),
        };
        Some(
            div()
                .absolute()
                .inset_0()
                .child(ui::backdrop("backdrop", alpha).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.close_overlay(window, cx))))
                .child(body)
                .into_any_element(),
        )
    }
}
