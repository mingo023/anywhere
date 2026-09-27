use crate::view::{basename, tilde};
use crate::{Desktop, Intent, Overlay};
use gpui_kit::component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::path::{Path, PathBuf};
use store::RepoConfig;
use theme::*;
use ui::{Segment, Variant};

#[derive(Clone, Copy, PartialEq)]
pub enum Mode {
    NewWorktree,
    Existing,
    Current,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Perm {
    Ask,
    AutoEdit,
    Plan,
}

pub struct NewForm {
    prompt: Entity<TextareaState>,
    branch: Entity<InputState>,
    mode: Mode,
    repo: Option<String>,
    branches: Vec<(String, Option<i64>)>,
    base: usize,
    current: String,
    existing: usize,
    copy_env: bool,
    run_setup: bool,
    provider: &'static str,
    perm: Perm,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Source {
    Local,
    Clone,
}

enum Probe {
    Loading,
    Git { branch: String, clean: bool, remotes: usize },
    NotGit,
}

pub struct RepoForm {
    source: Source,
    path: Option<String>,
    probe: Option<Probe>,
    url: Entity<InputState>,
    name: Entity<InputState>,
    setup: Entity<InputState>,
    color: u32,
    branches: Vec<String>,
    base: usize,
    worktrees: String,
    copy: Vec<String>,
    editing: Option<String>,
    busy: bool,
    error: Option<String>,
}

fn slug(prompt: &str) -> String {
    let words: Vec<String> = prompt.split(|c: char| !c.is_ascii_alphanumeric()).filter(|w| !w.is_empty()).take(4).map(str::to_lowercase).collect();
    if words.is_empty() { String::new() } else { format!("task/{}", words.join("-")) }
}

fn repo_from_url(url: &str) -> String {
    let tail = url.trim().trim_end_matches('/').rsplit(['/', ':']).next().unwrap_or_default();
    tail.strip_suffix(".git").unwrap_or(tail).to_string()
}

fn default_base<'a>(branches: impl Iterator<Item = &'a str> + Clone, preferred: &str, current: &str) -> usize {
    let at = |name: &str| branches.clone().position(|b| b == name);
    at(preferred).or_else(|| at("main")).or_else(|| at("master")).or_else(|| at(current)).unwrap_or(0)
}

fn home() -> String {
    std::env::var("HOME").unwrap_or_default()
}

fn typed_or(input: &Entity<InputState>, fallback: impl FnOnce() -> String, cx: &App) -> String {
    let v = input.read(cx).value().trim().to_string();
    if v.is_empty() { fallback() } else { v }
}

fn pick_card(id: &'static str, label: &str, value: impl IntoElement, chevron: bool) -> Stateful<Div> {
    div()
        .id(id)
        .flex_1()
        .min_w_0()
        .h(px(54.))
        .px(px(12.))
        .flex()
        .flex_col()
        .justify_center()
        .gap(px(3.))
        .rounded(px(12.))
        .bg(rgba(SURFACE))
        .shadow(vec![ui::ring(SEPARATOR, 0.5)])
        .when(chevron, |d| d.cursor_pointer())
        .child(div().text_size(px(11.5)).text_color(rgba(TEXT_3)).child(label.to_string()))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(6.))
                .text_size(px(13.5))
                .child(div().flex_1().min_w_0().flex().items_center().gap(px(6.)).overflow_hidden().child(value))
                .when(chevron, |d| d.child(icon("chevron-down", 12., TEXT_4))),
        )
}

fn check_item(id: &'static str, on: bool, label: &str) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .gap(px(6.))
        .cursor_pointer()
        .text_size(px(12.5))
        .text_color(rgba(TEXT_2))
        .child(ui::checkbox(on))
        .child(label.to_string())
}

fn field(label: &str, body: impl IntoElement) -> Div {
    div().flex_1().min_w_0().flex().flex_col().gap(px(6.)).child(ui::field_label(label.to_string())).child(body)
}

fn row_label(label: &str) -> Div {
    div().text_size(px(13.)).text_color(rgba(TEXT_3)).child(label.to_string())
}

fn footer(note: impl IntoElement, cancel: Stateful<Div>, submit: Stateful<Div>) -> Div {
    div().pt(px(4.)).flex().items_center().gap(px(8.)).child(div().flex_1().min_w_0().text_size(px(13.)).text_color(rgba(TEXT_4)).child(note)).child(cancel).child(submit)
}

impl NewForm {
    pub fn new(window: &mut Window, cx: &mut Context<Desktop>) -> (Self, Vec<Subscription>) {
        let prompt = cx.new(|cx| TextareaState::new(window, cx).placeholder("Describe the task…").rows(4));
        let branch = cx.new(|cx| InputState::new(window, cx));
        let subs = vec![
            cx.subscribe_in(&prompt, window, |this, prompt, ev: &InputEvent, window, cx| match ev {
                InputEvent::PressEnter { secondary: true, .. } => this.start_session(window, cx),
                InputEvent::Change => {
                    let slug = slug(&prompt.read(cx).value());
                    this.new_form.branch.update(cx, |b, cx| b.set_placeholder(slug, window, cx));
                    cx.notify();
                }
                _ => {}
            }),
            cx.subscribe(&branch, |_, _, _: &InputEvent, cx| cx.notify()),
        ];
        let form = Self {
            prompt,
            branch,
            mode: Mode::NewWorktree,
            repo: None,
            branches: Vec::new(),
            base: 0,
            current: String::new(),
            existing: 0,
            copy_env: false,
            run_setup: false,
            provider: "claude",
            perm: Perm::Ask,
        };
        (form, subs)
    }
}

impl RepoForm {
    pub fn new(window: &mut Window, cx: &mut Context<Desktop>) -> (Self, Vec<Subscription>) {
        let url = cx.new(|cx| InputState::new(window, cx).placeholder("https://github.com/org/repo.git"));
        let name = cx.new(|cx| InputState::new(window, cx));
        let setup = cx.new(|cx| InputState::new(window, cx).placeholder("pnpm install"));
        let subs = vec![
            cx.subscribe_in(&url, window, |this, url, ev: &InputEvent, window, cx| {
                if let InputEvent::Change = ev {
                    let name = repo_from_url(&url.read(cx).value());
                    this.repo_form.name.update(cx, |n, cx| n.set_placeholder(name, window, cx));
                }
                cx.notify();
            }),
            cx.subscribe(&name, |_, _, _: &InputEvent, cx| cx.notify()),
        ];
        let form = Self {
            source: Source::Local,
            path: None,
            probe: None,
            url,
            name,
            setup,
            color: PALETTE[0],
            branches: Vec::new(),
            base: 0,
            worktrees: String::new(),
            copy: Vec::new(),
            editing: None,
            busy: false,
            error: None,
        };
        (form, subs)
    }
}

impl Desktop {
    fn worktrees_dir(&self, repo: &str) -> String {
        let custom = self.store.repos.get(repo).map(|r| r.worktrees.clone()).filter(|w| !w.is_empty());
        custom.unwrap_or_else(|| format!("{}/.worktrees/{}", home(), self.repo_name(repo)))
    }

    /// Files copied into a new worktree: the repository's list, else its root `.env*` files.
    fn copy_list(&self, repo: &str) -> Vec<String> {
        let listed = self.store.repos.get(repo).map(|r| r.copy.clone()).unwrap_or_default();
        if !listed.is_empty() {
            return listed;
        }
        let entries = std::fs::read_dir(repo).into_iter().flatten().flatten();
        entries.filter_map(|e| e.file_name().into_string().ok()).filter(|n| n.starts_with(".env")).collect()
    }

    fn pick_path(&self, dirs: bool, window: &mut Window, cx: &mut Context<Self>, then: impl FnOnce(&mut Self, Vec<PathBuf>, &mut Window, &mut Context<Self>) + 'static) {
        let rx = cx.prompt_for_paths(PathPromptOptions { files: !dirs, directories: dirs, multiple: !dirs, prompt: None });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = rx.await else { return };
            this.update_in(cx, |d, window, cx| {
                then(d, paths, window, cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub fn reset_new_form(&mut self, prompt: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        let text = prompt.unwrap_or_default();
        let placeholder = slug(&text);
        let f = &mut self.new_form;
        f.prompt.update(cx, |s, cx| {
            s.set_value(text, window, cx);
            s.focus(window, cx);
        });
        f.branch.update(cx, |s, cx| {
            s.set_value("", window, cx);
            s.set_placeholder(placeholder, window, cx);
        });
        f.mode = Mode::NewWorktree;
        f.perm = Perm::Ask;
        if let Some(repo) = self.project.clone() {
            self.pick_repo(repo, cx);
        }
    }

    fn pick_repo(&mut self, repo: String, cx: &mut Context<Self>) {
        let cfg = self.store.repos.get(&repo).cloned().unwrap_or_default();
        let existing = self.worktrees.get(&repo).into_iter().flatten().filter(|w| !w.main).position(|w| self.worktree.as_ref() == Some(&w.path));
        let f = &mut self.new_form;
        f.repo = Some(repo.clone());
        f.branches.clear();
        f.current.clear();
        f.base = 0;
        f.existing = existing.unwrap_or(0);
        f.copy_env = !cfg.copy.is_empty();
        f.run_setup = !cfg.setup.is_empty();
        let dir = repo.clone();
        let task = cx.background_executor().spawn(async move {
            let current = git::read(&dir).map(|r| r.branch).unwrap_or_default();
            let branches: Vec<(String, Option<i64>)> = git::branches(&dir).into_iter().take(20).map(|b| {
                let at = git::committed_at(&dir, &b);
                (b, at)
            }).collect();
            (current, branches)
        });
        cx.spawn(async move |this, cx| {
            let (current, branches) = task.await;
            this.update(cx, |d, cx| {
                let f = &mut d.new_form;
                if f.repo.as_ref() != Some(&repo) {
                    return;
                }
                f.base = default_base(branches.iter().map(|(b, _)| b.as_str()), &cfg.base, &current);
                (f.current, f.branches) = (current, branches);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn new_branch(&self, cx: &App) -> String {
        typed_or(&self.new_form.branch, || slug(&self.new_form.prompt.read(cx).value()), cx)
    }

    fn repo_form_name(&self, cx: &App) -> String {
        let f = &self.repo_form;
        typed_or(
            &f.name,
            || match f.source {
                Source::Local => f.path.as_deref().map(basename).unwrap_or_default(),
                Source::Clone => repo_from_url(&f.url.read(cx).value()),
            },
            cx,
        )
    }

    fn existing_worktree(&self) -> Option<&git::Worktree> {
        let repo = self.new_form.repo.as_ref()?;
        self.worktrees.get(repo)?.iter().filter(|w| !w.main).nth(self.new_form.existing)
    }

    fn session_ready(&self, cx: &App) -> bool {
        match self.new_form.mode {
            _ if self.new_form.repo.is_none() => false,
            Mode::NewWorktree => !self.new_branch(cx).is_empty() && !self.new_form.branches.is_empty(),
            Mode::Existing => self.existing_worktree().is_some(),
            Mode::Current => true,
        }
    }

    fn start_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.session_ready(cx) {
            return;
        }
        let f = &self.new_form;
        let Some(repo) = f.repo.clone() else { return };
        let prompt = f.prompt.read(cx).value().trim().to_string();
        let mut args: Vec<String> = match (f.provider, f.perm) {
            (_, Perm::Ask) => vec![],
            ("claude", Perm::AutoEdit) => vec!["--permission-mode".into(), "acceptEdits".into()],
            ("claude", Perm::Plan) => vec!["--permission-mode".into(), "plan".into()],
            (_, Perm::AutoEdit) => vec!["--full-auto".into()],
            (_, Perm::Plan) => vec!["-s".into(), "read-only".into()],
        };
        args.extend((!prompt.is_empty()).then_some(prompt));
        let provider = f.provider;
        match f.mode {
            Mode::Current => self.send_spawn(daemon::spawn_argv(provider, args, &repo), Intent::Session, cx),
            Mode::Existing => {
                let Some(path) = self.existing_worktree().map(|w| w.path.clone()) else { return };
                self.send_spawn(daemon::spawn_argv(provider, args, &path), Intent::Session, cx);
            }
            Mode::NewWorktree => {
                let branch = self.new_branch(cx);
                let path = format!("{}/{}", self.worktrees_dir(&repo), branch.replace('/', "-"));
                let base = f.branches.get(f.base).map(|(b, _)| b.clone()).unwrap_or_default();
                let copy = if f.copy_env { self.copy_list(&repo) } else { Vec::new() };
                let setup = self.store.repos.get(&repo).map(|r| r.setup.clone()).filter(|_| f.run_setup).unwrap_or_default();
                let task = cx.background_executor().spawn(async move {
                    git::add_worktree(&repo, &path, &branch, &base)?;
                    for rel in copy {
                        let to = Path::new(&path).join(&rel);
                        if let Some(dir) = to.parent() {
                            std::fs::create_dir_all(dir).ok();
                        }
                        std::fs::copy(Path::new(&repo).join(&rel), to).ok();
                    }
                    if !setup.is_empty() {
                        let out = std::process::Command::new("sh").args(["-c", &setup]).current_dir(&path).output().map_err(|e| e.to_string())?;
                        if !out.status.success() {
                            return Err(format!("Setup failed: {}", String::from_utf8_lossy(&out.stderr).trim()));
                        }
                    }
                    Ok(path)
                });
                cx.spawn(async move |this, cx| {
                    let res = task.await;
                    this.update(cx, |d, cx| {
                        match res {
                            Ok(path) => d.send_spawn(daemon::spawn_argv(provider, args, &path), Intent::Session, cx),
                            Err(e) => d.error = Some(e),
                        }
                        d.refresh_git(cx);
                        cx.notify();
                    })
                    .ok();
                })
                .detach();
            }
        }
        self.close_overlay(window, cx);
    }

    pub fn new_worktree(&mut self, _: &crate::NewWorktree, window: &mut Window, cx: &mut Context<Self>) {
        self.open(Overlay::NewSession, window, cx);
    }

    pub fn project_settings(&mut self, _: &crate::ProjectSettings, window: &mut Window, cx: &mut Context<Self>) {
        self.overlay = Some(Overlay::AddRepo);
        self.reset_repo_form(self.project.clone(), window, cx);
        cx.notify();
    }

    pub fn new_session_view(&mut self, _: &mut Window, cx: &mut Context<Self>) -> Div {
        let f = &self.new_form;
        let repos = self.projects();
        let repo = f.repo.clone().unwrap_or_default();
        let ix = repos.iter().position(|p| *p == repo);
        let now = crate::view::now_ms();
        let prompt = div()
            .min_h(px(120.))
            .p(px(16.))
            .rounded(px(16.))
            .bg(rgba(SURFACE))
            .shadow(vec![ui::ring(SEPARATOR, 0.5), ui::shadow(0x0000000a, 1., 3.)])
            .text_size(px(15.))
            .line_height(px(23.))
            .child(Textarea::new(&f.prompt).appearance(false));
        let workspace = div().flex().items_center().child(row_label("Workspace")).child(div().flex_1()).child(
            div().id("workspace-mode").child(ui::segmented(
                vec![
                    Segment { icon: Some("worktree"), value: Mode::NewWorktree, label: "New worktree".into(), badge: None },
                    Segment { icon: None, value: Mode::Existing, label: "Existing worktree".into(), badge: None },
                    Segment { icon: None, value: Mode::Current, label: "Current checkout".into(), badge: None },
                ],
                f.mode,
                false,
                false,
                |this: &mut Self, v, cx| {
                    this.new_form.mode = v;
                    cx.notify();
                },
                cx,
            )),
        );
        let repo_value = div()
            .flex()
            .items_center()
            .gap(px(6.))
            .child(ui::swatch(self.repo_color(&repo), 14., 4.))
            .child(div().truncate().font_weight(FontWeight::SEMIBOLD).child(self.repo_name(&repo)))
            .children(ix.filter(|_| repos.len() > 1).map(|i| div().flex_none().text_size(px(12.)).text_color(rgba(TEXT_4)).child(format!("{} of {}", i + 1, repos.len()))));
        let next_repo = ix.map(|i| repos[(i + 1) % repos.len()].clone());
        let repo_card = pick_card("form-repo", "Repository", repo_value, repos.len() > 1)
            .when_some(next_repo.filter(|_| repos.len() > 1), |d, next| d.on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.pick_repo(next.clone(), cx);
                cx.notify();
            })));
        let branch_mark = |name: String, at: Option<i64>| {
            div()
                .flex()
                .items_center()
                .gap(px(6.))
                .child(icon("branch", 12., TEXT_3))
                .child(div().truncate().font_family(MONO).text_size(px(13.)).child(name))
                .children(at.map(|s| div().flex_none().text_size(px(12.)).text_color(rgba(TEXT_4)).child(format!("· {}", crate::view::ago_long(s * 1000, now)))))
        };
        let cards = match f.mode {
            Mode::NewWorktree => {
                let (base, at) = f.branches.get(f.base).cloned().unwrap_or_default();
                let count = f.branches.len();
                let base_card = pick_card("form-base", "Base branch", branch_mark(base, at), count > 1).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.new_form.base = (this.new_form.base + 1) % count.max(1);
                    cx.notify();
                }));
                let typed = !f.branch.read(cx).value().is_empty();
                let branch = div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .w_full()
                    .child(div().flex_1().min_w_0().font_family(MONO).child(Input::new(&f.branch).appearance(false).p_0().text_size(px(13.))))
                    .when(!typed, |d| d.child(div().flex_none().text_size(px(11.5)).text_color(rgba(TEXT_4)).child("from task")));
                vec![repo_card, base_card, pick_card("form-branch", "New branch", branch, false)]
            }
            Mode::Existing => {
                let trees: Vec<git::Worktree> = self.worktrees.get(&repo).into_iter().flatten().filter(|w| !w.main).cloned().collect();
                let count = trees.len();
                let value = match trees.get(f.existing) {
                    Some(w) => branch_mark(w.branch.clone(), None).child(div().truncate().text_size(px(12.)).text_color(rgba(TEXT_4)).child(basename(&w.path))),
                    None => div().text_color(rgba(TEXT_4)).child("No worktrees yet"),
                };
                let card = pick_card("form-existing", "Worktree", value, count > 1).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.new_form.existing = (this.new_form.existing + 1) % count.max(1);
                    cx.notify();
                }));
                vec![repo_card, card]
            }
            Mode::Current => vec![repo_card, pick_card("form-current", "Branch", branch_mark(f.current.clone(), None), false)],
        };
        let path = match f.mode {
            Mode::NewWorktree => Some(format!("{}/{}", self.worktrees_dir(&repo), self.new_branch(cx).replace('/', "-"))),
            Mode::Existing => self.existing_worktree().map(|w| w.path.clone()),
            Mode::Current => Some(repo.clone()),
        };
        let has_setup = self.store.repos.get(&repo).is_some_and(|r| !r.setup.is_empty());
        let path_row = path.map(|p| {
            div()
                .h(px(34.))
                .px(px(12.))
                .flex()
                .items_center()
                .gap(px(10.))
                .rounded(px(10.))
                .bg(rgba(FILL_2))
                .child(icon("folder", 13., TEXT_3))
                .child(div().flex_1().min_w_0().truncate().font_family(MONO).text_size(px(12.5)).text_color(rgba(TEXT_BODY)).child(tilde(&p)))
                .when(f.mode == Mode::NewWorktree, |d| {
                    d.child(check_item("form-copy", f.copy_env, "Copy .env files").on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.new_form.copy_env = !this.new_form.copy_env;
                        cx.notify();
                    })))
                    .when(has_setup, |d| {
                        d.child(check_item("form-setup", f.run_setup, "Run setup").on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                            this.new_form.run_setup = !this.new_form.run_setup;
                            cx.notify();
                        })))
                    })
                })
        });
        let model = |provider: &str| {
            let latest = self.agents.list.iter().filter(|a| a.provider == provider && a.model.is_some()).max_by_key(|a| a.updated_at);
            latest.map(agents::model_label).unwrap_or_else(|| "Default model".into())
        };
        let agent = |id: &'static str, provider: &'static str, name: &str, color: u32| {
            let selected = f.provider == provider;
            div()
                .id(id)
                .flex_1()
                .h(px(60.))
                .px(px(14.))
                .flex()
                .flex_col()
                .justify_center()
                .gap(px(2.))
                .rounded(px(14.))
                .cursor_pointer()
                .bg(rgba(SURFACE))
                .shadow(vec![if selected { ui::ring(color, 1.5) } else { ui::ring(SEPARATOR, 0.5) }])
                .child(div().flex().items_center().gap(px(8.)).child(ui::dot(8., color)).child(div().text_size(px(14.5)).font_weight(FontWeight::SEMIBOLD).child(name.to_string())))
                .child(div().text_size(px(12.5)).text_color(rgba(TEXT_3)).child(model(provider)))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.new_form.provider = provider;
                    cx.notify();
                }))
        };
        let agents = div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(row_label("Agent"))
            .child(div().flex().gap(px(8.)).child(agent("form-claude", "claude", "Claude Code", AGENT_CLAUDE)).child(agent("form-codex", "codex", "Codex", AGENT_CODEX)));
        let perms = div().flex().items_center().gap(px(12.)).child(row_label("Permissions")).child(
            div().id("form-perm").child(ui::segmented(
                vec![
                    Segment { icon: None, value: Perm::Ask, label: "Ask".into(), badge: None },
                    Segment { icon: None, value: Perm::AutoEdit, label: "Auto-edit".into(), badge: None },
                    Segment { icon: None, value: Perm::Plan, label: "Plan only".into(), badge: None },
                ],
                f.perm,
                false,
                false,
                |this: &mut Self, v, cx| {
                    this.new_form.perm = v;
                    cx.notify();
                },
                cx,
            )),
        );
        let ready = self.session_ready(cx);
        let cancel = ui::large(ui::button("form-cancel", Variant::Ghost, None, "Cancel").text_color(rgba(TEXT)))
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.close_overlay(window, cx)));
        let start = ui::large(ui::button("form-start", Variant::Primary, None, "Start session"))
            .child(ui::button_kbd("⌘↵"))
            .when(ready, |d| d.on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.start_session(window, cx))))
            .when(!ready, |d| d.opacity(0.5).cursor_default());
        let close = ui::icon_button("form-close", "x").on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.close_overlay(window, cx)));
        ui::modal(
            "New session",
            700.,
            70.,
            close,
            [
                prompt.into_any_element(),
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(workspace)
                    .child(div().flex().gap(px(8.)).children(cards))
                    .children(path_row)
                    .into_any_element(),
                agents.into_any_element(),
                perms.into_any_element(),
                footer("Opens in a new terminal tab", cancel, start).into_any_element(),
            ],
        )
    }

    pub fn reset_repo_form(&mut self, editing: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        let cfg = editing.as_ref().and_then(|p| self.store.repos.get(p)).cloned().unwrap_or_default();
        let color = match &editing {
            Some(p) => self.repo_color(p),
            None => PALETTE[self.projects().len() % PALETTE.len()],
        };
        let f = &mut self.repo_form;
        f.source = Source::Local;
        f.path = None;
        f.probe = None;
        f.color = color;
        f.branches.clear();
        f.base = 0;
        f.worktrees = cfg.worktrees;
        f.copy = cfg.copy;
        f.editing = editing.clone();
        f.busy = false;
        f.error = None;
        f.url.update(cx, |s, cx| s.set_value("", window, cx));
        f.name.update(cx, |s, cx| {
            s.set_value(cfg.name, window, cx);
            s.set_placeholder("", window, cx);
        });
        f.setup.update(cx, |s, cx| s.set_value(cfg.setup, window, cx));
        if let Some(p) = editing {
            self.set_repo_path(p, window, cx);
        }
    }

    fn set_repo_path(&mut self, path: String, window: &mut Window, cx: &mut Context<Self>) {
        let base = self.store.repos.get(&path).map(|r| r.base.clone()).unwrap_or_default();
        let f = &mut self.repo_form;
        f.path = Some(path.clone());
        if f.source == Source::Clone {
            return;
        }
        f.probe = Some(Probe::Loading);
        f.name.update(cx, |s, cx| s.set_placeholder(basename(&path), window, cx));
        let dir = path.clone();
        let task = cx.background_executor().spawn(async move { (git::read(&dir), git::branches(&dir), git::remotes(&dir)) });
        cx.spawn(async move |this, cx| {
            let (repo, branches, remotes) = task.await;
            this.update(cx, |d, cx| {
                let f = &mut d.repo_form;
                if f.path.as_ref() != Some(&path) {
                    return;
                }
                let current = repo.as_ref().map(|r| r.branch.as_str()).unwrap_or_default();
                f.base = default_base(branches.iter().map(String::as_str), &base, current);
                f.probe = Some(match repo {
                    Some(r) => Probe::Git { clean: r.files.is_empty(), branch: r.branch, remotes },
                    None => Probe::NotGit,
                });
                f.branches = branches;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn repo_ready(&self, cx: &App) -> bool {
        let f = &self.repo_form;
        let named = !self.repo_form_name(cx).is_empty();
        match f.source {
            _ if f.busy => false,
            Source::Local => named && matches!(f.probe, Some(Probe::Git { .. })),
            Source::Clone => named && f.path.is_some() && !f.url.read(cx).value().trim().is_empty(),
        }
    }

    fn save_repo(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.repo_ready(cx) {
            return;
        }
        let f = &self.repo_form;
        let Some(path) = f.path.clone() else { return };
        let name = self.repo_form_name(cx);
        let cfg = RepoConfig {
            name: name.clone(),
            color: f.color,
            base: f.branches.get(f.base).cloned().unwrap_or_default(),
            worktrees: f.worktrees.clone(),
            setup: f.setup.read(cx).value().trim().to_string(),
            copy: f.copy.clone(),
        };
        if f.source == Source::Local {
            return self.add_repo(path, cfg, window, cx);
        }
        let url = f.url.read(cx).value().trim().to_string();
        let dest = format!("{path}/{name}");
        self.repo_form.busy = true;
        self.repo_form.error = None;
        let task = cx.background_executor().spawn({
            let dest = dest.clone();
            async move { git::clone(&url, &dest) }
        });
        cx.spawn_in(window, async move |this, cx| {
            let res = task.await;
            this.update_in(cx, |d, window, cx| {
                d.repo_form.busy = false;
                match res {
                    Ok(()) => d.add_repo(dest, cfg, window, cx),
                    Err(e) => d.repo_form.error = Some(e),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn add_repo(&mut self, path: String, cfg: RepoConfig, window: &mut Window, cx: &mut Context<Self>) {
        if !self.store.projects.contains(&path) {
            self.store.projects.push(path.clone());
        }
        self.store.repos.insert(path.clone(), cfg);
        self.store.save();
        self.close_overlay(window, cx);
        self.select_project(path, cx);
        self.refresh_git(cx);
    }

    fn folder_card(&self, cx: &mut Context<Self>) -> Div {
        let f = &self.repo_form;
        let clone = f.source == Source::Clone;
        let title = match (&f.path, clone) {
            (Some(p), true) => format!("Clone into {}", tilde(p)),
            (Some(p), false) => tilde(p),
            (None, true) => "Choose where to clone".into(),
            (None, false) => "No folder chosen".into(),
        };
        let (mark, line, color) = match &f.probe {
            _ if clone => (None, "The repository is cloned into a folder named after it.".to_string(), TEXT_4),
            None => (None, "Pick the folder of a git repository.".into(), TEXT_4),
            Some(Probe::Loading) => (None, "Checking…".into(), TEXT_4),
            Some(Probe::NotGit) => (Some("x"), "Not a git repository".into(), FAILED),
            Some(Probe::Git { branch, clean, remotes }) => {
                let state = if *clean { "clean" } else { "uncommitted changes" };
                let remotes = match remotes {
                    1 => "1 remote".to_string(),
                    n => format!("{n} remotes"),
                };
                (Some("check"), format!("Git repository · {branch} · {state} · {remotes}"), RUNNING_TEXT)
            }
        };
        div()
            .h(px(60.))
            .px(px(14.))
            .flex()
            .items_center()
            .gap(px(12.))
            .rounded(px(14.))
            .bg(rgba(SURFACE))
            .shadow(vec![ui::ring(SEPARATOR, 0.5)])
            .child(div().size(px(36.)).flex().flex_none().items_center().justify_center().rounded(px(10.)).bg(rgba(FILL_2)).child(icon("folder", 16., TEXT_2)))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(div().truncate().font_family(MONO).text_size(px(13.5)).font_weight(FontWeight::SEMIBOLD).child(title))
                    .child(div().flex().items_center().gap(px(4.)).text_size(px(12.5)).text_color(rgba(color)).children(mark.map(|m| icon(m, 12., color))).child(line)),
            )
            .when(f.editing.is_none(), |d| {
                d.child(ui::button("repo-choose", Variant::Secondary, None, "Choose…").on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                    this.pick_path(true, window, cx, |d, paths, window, cx| {
                        if let Some(p) = paths.into_iter().next() {
                            d.set_repo_path(p.to_string_lossy().into_owned(), window, cx);
                        }
                    })
                })))
            })
    }

    pub fn repo_view(&mut self, _: &mut Window, cx: &mut Context<Self>) -> Div {
        let name = self.repo_form_name(cx);
        let f = &self.repo_form;
        let source = div().id("repo-source").child(ui::segmented(
            vec![Segment { icon: Some("folder"), value: Source::Local, label: "Local folder".into(), badge: None }, Segment { icon: Some("external"), value: Source::Clone, label: "Clone from URL".into(), badge: None }],
            f.source,
            false,
            true,
            |this: &mut Self, v, cx| {
                let f = &mut this.repo_form;
                f.source = v;
                f.path = (v == Source::Clone).then(|| format!("{}/code", home())).filter(|p| Path::new(p).is_dir());
                f.probe = None;
                f.branches.clear();
                cx.notify();
            },
            cx,
        ));
        let url = (f.source == Source::Clone).then(|| {
            ui::field_box().child(icon("external", 13., TEXT_3)).child(div().flex_1().font_family(MONO).child(Input::new(&f.url).appearance(false).p_0().text_size(px(13.))))
        });
        let colors = div().h(px(38.)).flex().items_center().gap(px(4.)).children(PALETTE.iter().enumerate().map(|(i, &c)| {
            div()
                .id(("repo-color", i))
                .size(px(28.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(9.))
                .cursor_pointer()
                .when(c == f.color, |d| d.shadow(vec![ui::ring(c, 1.5)]))
                .child(ui::swatch(c, 22., 7.))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.repo_form.color = c;
                    cx.notify();
                }))
        }));
        let identity = div()
            .flex()
            .gap(px(14.))
            .child(field("Name", ui::field_box().child(div().flex_1().child(Input::new(&f.name).appearance(false).p_0().text_size(px(14.))))))
            .child(field("Colour", colors));
        let count = f.branches.len();
        let base_name = f.branches.get(f.base).cloned().unwrap_or_else(|| "Default branch".into());
        let base = ui::field_box()
            .id("repo-base")
            .when(count > 1, |d| d.cursor_pointer())
            .child(icon("branch", 13., TEXT_3))
            .child(div().flex_1().min_w_0().truncate().font_family(MONO).text_size(px(13.)).child(base_name))
            .child(icon("chevron-down", 12., TEXT_4))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.repo_form.base = (this.repo_form.base + 1) % count.max(1);
                cx.notify();
            }));
        let folder = if f.worktrees.is_empty() { format!("~/.worktrees/{name}") } else { tilde(&f.worktrees) };
        let worktrees = ui::field_box()
            .child(icon("folder", 13., TEXT_3))
            .child(div().flex_1().min_w_0().truncate().font_family(MONO).text_size(px(13.)).child(folder))
            .child(ui::link("repo-worktrees", "Change").on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                this.pick_path(true, window, cx, |d, paths, _, _| {
                    if let Some(p) = paths.into_iter().next() {
                        d.repo_form.worktrees = p.to_string_lossy().into_owned();
                    }
                })
            })));
        let layout = div()
            .flex()
            .gap(px(14.))
            .child(field("Default base branch", base).child(div().text_size(px(12.)).text_color(rgba(TEXT_4)).child("New worktrees branch off this unless you pick another.")))
            .child(field("Worktrees folder", worktrees));
        let setup = field(
            "When a worktree is created",
            ui::field_box().child(icon("terminal", 13., TEXT_3)).child(div().flex_1().font_family(MONO).child(Input::new(&f.setup).appearance(false).p_0().text_size(px(13.)))),
        );
        let repo = f.path.clone().filter(|_| f.source == Source::Local);
        let copies = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(6.))
            .child(div().mr(px(2.)).text_size(px(12.5)).text_color(rgba(TEXT_3)).child("Copy into each worktree"))
            .children(f.copy.iter().enumerate().map(|(i, rel)| {
                ui::tag(rel.clone())
                    .id(("repo-copy", i))
                    .gap(px(4.))
                    .cursor_pointer()
                    .font_family(MONO)
                    .text_color(rgba(TEXT))
                    .child(icon("x", 9., TEXT_4))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.repo_form.copy.remove(i);
                        cx.notify();
                    }))
            }))
            .when_some(repo, |d, repo| {
                d.child(ui::link("repo-add-file", "+ Add file").on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    let repo = repo.clone();
                    this.pick_path(false, window, cx, move |d, paths, _, _| {
                        for p in paths {
                            let Ok(rel) = p.strip_prefix(&repo) else { continue };
                            let rel = rel.to_string_lossy().into_owned();
                            if !d.repo_form.copy.contains(&rel) {
                                d.repo_form.copy.push(rel);
                            }
                        }
                    })
                })))
            });
        let editing = f.editing.is_some();
        let note = match &f.error {
            Some(e) => div().truncate().text_color(rgba(FAILED)).child(e.clone()),
            None if editing || name.is_empty() => div(),
            None => div().truncate().child(format!("Adds {name} to the project rail")),
        };
        let ready = self.repo_ready(cx);
        let label = match () {
            _ if f.busy => "Cloning…",
            _ if editing => "Save",
            _ => "Add repository",
        };
        let cancel = ui::large(ui::button("repo-cancel", Variant::Ghost, None, "Cancel").text_color(rgba(TEXT)))
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.close_overlay(window, cx)));
        let submit = ui::large(ui::button("repo-submit", Variant::Primary, None, label))
            .when(ready, |d| d.on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.save_repo(window, cx))))
            .when(!ready, |d| d.opacity(0.5).cursor_default());
        let close = ui::icon_button("repo-close", "x").on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.close_overlay(window, cx)));
        let title = if editing { "Repository settings" } else { "Add repository" };
        let mut body: Vec<AnyElement> = Vec::new();
        if !editing {
            body.push(source.into_any_element());
        }
        body.extend(url.map(IntoElement::into_any_element));
        body.extend([
            self.folder_card(cx).into_any_element(),
            identity.into_any_element(),
            layout.into_any_element(),
            div().flex().flex_col().gap(px(10.)).child(setup).child(copies).into_any_element(),
            footer(note, cancel, submit).into_any_element(),
        ]);
        ui::modal(title, 640., 64., close, body)
    }
}

#[cfg(test)]
mod tests {
    use super::{repo_from_url, slug};

    #[test]
    fn slug_names_a_branch_after_the_prompt() {
        assert_eq!(slug("The RestoreView snapshot fails on CI"), "task/the-restoreview-snapshot-fails");
        assert_eq!(slug("fix: flaky!"), "task/fix-flaky");
        assert_eq!(slug("  …  "), "");
    }

    #[test]
    fn repo_name_comes_from_the_url() {
        assert_eq!(repo_from_url("https://github.com/org/app-ios.git"), "app-ios");
        assert_eq!(repo_from_url("git@github.com:org/app-ios.git"), "app-ios");
        assert_eq!(repo_from_url("https://github.com/org/web/ "), "web");
    }
}
