use crate::view::{basename, tilde};
use crate::{Desktop, Intent, Overlay};
use gpui_kit::component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use store::RepoConfig;
use theme::*;
use ui::{Segment, Variant};

#[derive(Clone, Copy, PartialEq)]
pub enum Perm {
    Ask,
    AutoEdit,
    Plan,
}

pub struct NewForm {
    prompt: Entity<TextareaState>,
    name: Entity<InputState>,
    /// Branches and worktree folders a new worktree's name must not reuse.
    taken: HashSet<String>,
    seed: usize,
    worktree: bool,
    repo: Option<String>,
    branches: Vec<(String, Option<i64>)>,
    base: usize,
    copy_env: bool,
    run_setup: bool,
    provider: &'static str,
    perm: Perm,
    picker: Option<Picker>,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Picker {
    Agent,
    Branch,
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
    prompt.split(|c: char| !c.is_ascii_alphanumeric()).filter(|w| !w.is_empty()).take(4).map(str::to_lowercase).collect::<Vec<_>>().join("-")
}

const ADJECTIVES: [&str; 8] = ["brave", "calm", "eager", "fuzzy", "keen", "lucky", "quiet", "swift"];
const NOUNS: [&str; 8] = ["otter", "heron", "maple", "comet", "falcon", "cedar", "koala", "lynx"];

/// Case-insensitive because APFS is; a branch `a/b` owns the name `a`.
fn is_taken(name: &str, taken: &HashSet<String>) -> bool {
    taken.iter().any(|t| t.split('/').next().unwrap_or(t).eq_ignore_ascii_case(name))
}

fn free_name(seed: usize, taken: &HashSet<String>) -> String {
    let count = ADJECTIVES.len() * NOUNS.len();
    (0..count).map(|i| (seed + i) % count).map(|n| format!("{}-{}", ADJECTIVES[n / NOUNS.len()], NOUNS[n % NOUNS.len()])).find(|n| !is_taken(n, taken)).unwrap_or_else(|| unique("worktree", taken))
}

/// `base`, or `base-2`, `base-3`… when taken.
fn unique(base: &str, taken: &HashSet<String>) -> String {
    std::iter::once(base.to_string()).chain((2..).map(|n| format!("{base}-{n}"))).find(|n| !is_taken(n, taken)).unwrap()
}

/// The name a worktree gets when the user leaves Name empty: the prompt's slug, else a random one.
fn auto_name(prompt: &str, seed: usize, taken: &HashSet<String>) -> String {
    match slug(prompt) {
        s if s.is_empty() => free_name(seed, taken),
        s => unique(&s, taken),
    }
}

/// Why `name` can't name a new worktree and its branch, if it can't.
fn name_problem(name: &str, taken: &HashSet<String>) -> Option<&'static str> {
    let valid = !name.is_empty()
        && name.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
        && !name.starts_with(['-', '.'])
        && !name.ends_with('.')
        && !name.ends_with(".lock")
        && !name.contains("..")
        && name != "HEAD";
    if is_taken(name, taken) {
        Some("A worktree or branch with this name already exists")
    } else if !valid {
        Some("Use letters, digits, - _ or .")
    } else {
        None
    }
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

fn chip(id: &'static str, open: bool) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(30.))
        .pl(px(10.))
        .pr(px(8.))
        .flex()
        .flex_none()
        .items_center()
        .gap(px(7.))
        .rounded(px(8.))
        .whitespace_nowrap()
        .cursor_pointer()
        .bg(rgba(if open { FILL_3 } else { FILL_2 }))
        .hover(|s| s.bg(rgba(FILL_3)))
}

fn pick_head(label: &str) -> Div {
    div().pt(px(8.)).px(px(8.)).pb(px(4.)).text_size(px(11.5)).font_weight(FontWeight::SEMIBOLD).text_color(rgba(TEXT_3)).child(label.to_string())
}

fn pick_row(id: impl Into<ElementId>, selected: bool, lead: Option<impl IntoElement>, label: Div, meta: Option<String>) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(34.))
        .px(px(8.))
        .flex()
        .flex_none()
        .items_center()
        .gap(px(9.))
        .rounded(px(6.))
        .cursor_pointer()
        .text_size(px(13.))
        .when(selected, |d| d.bg(rgba(FILL_2)))
        .when(!selected, |d| d.hover(|s| s.bg(rgba(FILL_2))))
        .children(lead)
        .child(label.min_w_0().truncate().font_weight(FontWeight::MEDIUM))
        .child(div().ml_auto().pl(px(10.)).flex_none().text_size(px(12.)).text_color(rgba(TEXT_4)).children(meta))
        .child(div().w(px(16.)).flex().flex_none().justify_end().when(selected, |d| d.child(icon("check", 14., TEXT))))
}

fn picker_menu(id: &'static str, width: f32, rows: Vec<AnyElement>, cx: &mut Context<Desktop>) -> Stateful<Div> {
    ui::pop(div().id(id))
        .w(px(width))
        .max_h(px(360.))
        .overflow_y_scroll()
        .p(px(5.))
        .rounded(px(10.))
        .flex()
        .flex_col()
        .children(rows)
        .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
            this.new_form.picker = None;
            cx.notify();
        }))
}

fn field(label: &str, body: impl IntoElement) -> Div {
    div().flex_1().min_w_0().flex().flex_col().gap(px(6.)).child(ui::field_label(label.to_string())).child(body)
}

pub fn footer(note: impl IntoElement, cancel: Stateful<Div>, submit: Stateful<Div>) -> Div {
    div().pt(px(4.)).flex().items_center().gap(px(8.)).child(div().flex_1().min_w_0().text_size(px(13.)).text_color(rgba(TEXT_4)).child(note)).child(cancel).child(submit)
}

impl NewForm {
    pub fn new(window: &mut Window, cx: &mut Context<Desktop>) -> (Self, Vec<Subscription>) {
        let prompt = cx.new(|cx| TextareaState::new(window, cx).placeholder("Describe what the agent should do…").rows(4));
        let name = cx.new(|cx| InputState::new(window, cx));
        let subs = vec![
            cx.subscribe_in(&prompt, window, |this, prompt, ev: &InputEvent, window, cx| match ev {
                InputEvent::PressEnter { secondary: true, .. } => this.start_session(window, cx),
                InputEvent::Change => {
                    let f = &this.new_form;
                    let name = auto_name(&prompt.read(cx).value(), f.seed, &f.taken);
                    this.new_form.name.update(cx, |b, cx| b.set_placeholder(name, window, cx));
                    cx.notify();
                }
                _ => {}
            }),
            cx.subscribe_in(&name, window, |this, _, ev: &InputEvent, window, cx| match ev {
                InputEvent::PressEnter { secondary: true, .. } => this.start_session(window, cx),
                _ => cx.notify(),
            }),
        ];
        let form = Self {
            prompt,
            name,
            taken: HashSet::new(),
            seed: 0,
            worktree: false,
            repo: None,
            branches: Vec::new(),
            base: 0,
            copy_env: false,
            run_setup: false,
            provider: "claude",
            perm: Perm::Ask,
            picker: None,
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

    pub fn reset_new_form(&mut self, prompt: Option<String>, worktree: bool, window: &mut Window, cx: &mut Context<Self>) {
        let text = prompt.unwrap_or_default();
        let f = &mut self.new_form;
        f.seed = crate::view::now_ms() as usize;
        f.taken.clear();
        let placeholder = auto_name(&text, f.seed, &f.taken);
        f.prompt.update(cx, |s, cx| {
            s.set_value(text, window, cx);
            s.focus(window, cx);
        });
        f.name.update(cx, |s, cx| {
            s.set_value("", window, cx);
            s.set_placeholder(placeholder, window, cx);
        });
        f.worktree = worktree;
        f.perm = Perm::Ask;
        f.picker = None;
        match self.project.clone() {
            Some(repo) => self.pick_repo(repo, window, cx),
            None => {
                f.repo = None;
                f.branches.clear();
            }
        }
    }

    fn pick_repo(&mut self, repo: String, window: &mut Window, cx: &mut Context<Self>) {
        let cfg = self.store.repos.get(&repo).cloned().unwrap_or_default();
        let folders = self.worktrees_dir(&repo);
        let f = &mut self.new_form;
        f.repo = Some(repo.clone());
        f.branches.clear();
        f.base = 0;
        f.copy_env = !cfg.copy.is_empty();
        f.run_setup = !cfg.setup.is_empty();
        let dir = repo.clone();
        let task = cx.background_executor().spawn(async move {
            let current = git::read(&dir).map(|r| r.branch).unwrap_or_default();
            let all = git::branches(&dir);
            let branches: Vec<(String, Option<i64>)> = all.iter().take(20).map(|b| {
                let at = git::committed_at(&dir, b);
                (b.clone(), at)
            }).collect();
            let entries = std::fs::read_dir(&folders).into_iter().flatten().flatten();
            let taken: HashSet<String> = all.into_iter().chain(entries.filter_map(|e| e.file_name().into_string().ok())).collect();
            (current, branches, taken)
        });
        cx.spawn_in(window, async move |this, cx| {
            let (current, mut branches, taken) = task.await;
            this.update_in(cx, |d, window, cx| {
                let f = &mut d.new_form;
                if f.repo.as_ref() != Some(&repo) {
                    return;
                }
                let default = default_base(branches.iter().map(|(b, _)| b.as_str()), &cfg.base, &current);
                if !branches.is_empty() {
                    branches[..=default].rotate_right(1);
                }
                f.base = 0;
                f.branches = branches;
                f.taken = taken;
                let name = auto_name(&f.prompt.read(cx).value(), f.seed, &f.taken);
                f.name.update(cx, |s, cx| s.set_placeholder(name, window, cx));
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn new_name(&self, cx: &App) -> String {
        let f = &self.new_form;
        typed_or(&f.name, || auto_name(&f.prompt.read(cx).value(), f.seed, &f.taken), cx)
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

    fn session_ready(&self, cx: &App) -> bool {
        let f = &self.new_form;
        if f.worktree {
            f.repo.is_some() && !f.branches.is_empty() && name_problem(&self.new_name(cx), &f.taken).is_none()
        } else {
            self.cwd().is_some()
        }
    }

    fn start_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.session_ready(cx) {
            return;
        }
        let f = &self.new_form;
        let prompt = f.prompt.read(cx).value().trim().to_string();
        let mut args: Vec<String> = match (f.provider, f.perm) {
            (_, Perm::Ask) => vec![],
            ("claude", Perm::AutoEdit) => vec!["--permission-mode".into(), "acceptEdits".into()],
            ("claude", Perm::Plan) => vec!["--permission-mode".into(), "plan".into()],
            (_, Perm::AutoEdit) => vec!["--full-auto".into()],
            (_, Perm::Plan) => vec!["-s".into(), "read-only".into()],
        };
        args.extend((!prompt.is_empty()).then_some(prompt));
        let argv: Vec<String> = std::iter::once(f.provider.to_string()).chain(args).collect();
        let worktree = f.worktree;
        match self.cwd().filter(|_| !worktree) {
            Some(tree) => self.send_spawn(daemon::agent_op(&argv, &tree), Intent::Tab(tree), cx),
            None => {
                let f = &self.new_form;
                let Some(repo) = f.repo.clone() else { return };
                let name = self.new_name(cx);
                let path = format!("{}/{name}", self.worktrees_dir(&repo));
                let base = f.branches.get(f.base).map(|(b, _)| b.clone()).unwrap_or_default();
                let copy = if f.copy_env { self.copy_list(&repo) } else { Vec::new() };
                let setup = self.store.repos.get(&repo).map(|r| r.setup.clone()).filter(|_| f.run_setup).unwrap_or_default();
                let project = repo.clone();
                let task = cx.background_executor().spawn(async move {
                    git::add_worktree(&repo, &path, &name, &base)?;
                    for rel in copy {
                        let to = Path::new(&path).join(&rel);
                        if let Some(dir) = to.parent() {
                            std::fs::create_dir_all(dir).ok();
                        }
                        std::fs::copy(Path::new(&repo).join(&rel), to).ok();
                    }
                    Ok::<_, String>(path)
                });
                cx.spawn(async move |this, cx| {
                    let res = task.await;
                    this.update(cx, |d, cx| {
                        if res.is_ok() && d.store.collapsed.remove(&project) {
                            d.store.save();
                        }
                        match res {
                            Ok(path) if setup.is_empty() => d.send_spawn(daemon::agent_op(&argv, &path), Intent::Tab(path), cx),
                            Ok(path) => d.send_spawn(daemon::setup_op(&setup, &argv, &path), Intent::Setup(path), cx),
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
        self.close_menus();
        self.overlay = Some(Overlay::NewSession);
        self.reset_new_form(None, true, window, cx);
        cx.notify();
    }

    pub fn new_worktree_in(&mut self, p: String, window: &mut Window, cx: &mut Context<Self>) {
        self.select_project(p, cx);
        self.new_worktree(&crate::NewWorktree, window, cx);
    }

    pub fn project_settings(&mut self, _: &crate::ProjectSettings, window: &mut Window, cx: &mut Context<Self>) {
        self.close_menus();
        self.overlay = Some(Overlay::AddRepo);
        self.reset_repo_form(self.project.clone(), window, cx);
        cx.notify();
    }

    pub fn close_picker(&mut self) -> bool {
        self.new_form.picker.take().is_some()
    }

    fn toggle_picker(&mut self, picker: Picker, cx: &mut Context<Self>) {
        let f = &mut self.new_form;
        f.picker = (f.picker != Some(picker)).then_some(picker);
        cx.notify();
    }

    fn agent_picker(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let f = &self.new_form;
        let mut rows = Vec::new();
        for provider in ["claude", "codex"] {
            let model = self.model_hint(provider).unwrap_or_else(|| "Default model".into());
            rows.push(pick_head(provider_name(provider)).into_any_element());
            rows.push(
                pick_row(provider, f.provider == provider, Some(ui::dot(7., provider_color(provider))), div().child(model), None)
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        (this.new_form.provider, this.new_form.picker) = (provider, None);
                        cx.notify();
                    }))
                    .into_any_element(),
            );
        }
        rows.push(pick_head("Permissions").into_any_element());
        for (perm, label) in [(Perm::Ask, "Ask"), (Perm::AutoEdit, "Auto-edit"), (Perm::Plan, "Plan only")] {
            rows.push(
                pick_row(label, f.perm == perm, None::<Div>, div().child(label), None)
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        (this.new_form.perm, this.new_form.picker) = (perm, None);
                        cx.notify();
                    }))
                    .into_any_element(),
            );
        }
        picker_menu("agent-menu", 260., rows, cx)
    }

    fn branch_picker(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let f = &self.new_form;
        let now = crate::view::now_ms();
        let branch = |name: &str| div().font_family(MONO).text_size(px(12.5)).child(name.to_string());
        let mut rows = Vec::new();
        for (i, (name, at)) in f.branches.iter().enumerate() {
            match i {
                0 => rows.push(pick_head("Default").into_any_element()),
                1 => rows.push(pick_head("Recent").into_any_element()),
                _ => {}
            }
            let meta = at.map(|s| crate::view::ago_long(s * 1000, now));
            rows.push(
                pick_row(("base", i), f.base == i, Some(icon("branch", 13., TEXT_3)), branch(name), meta)
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        (this.new_form.base, this.new_form.picker) = (i, None);
                        cx.notify();
                    }))
                    .into_any_element(),
            );
        }
        picker_menu("branch-menu", 300., rows, cx)
    }

    pub fn new_session_view(&mut self, _: &mut Window, cx: &mut Context<Self>) -> Div {
        let f = &self.new_form;
        let repo = f.repo.clone().unwrap_or_default();
        let name = self.repo_name(&repo);
        let close = ui::icon_button_sized("form-close", "x", 28., TEXT_3).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.close_overlay(window, cx)));
        let header = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .px(px(4.))
            .pb(px(2.))
            .child(div().text_size(px(16.)).font_weight(FontWeight::BOLD).child(if f.worktree { "New worktree" } else { "New session" }))
            .child(div().flex().items_center().gap(px(6.)).text_size(px(13.)).text_color(rgba(TEXT_3)).child(ui::repo_tile(&crate::view::initials(&name), 18., false, None)).child(name))
            .child(div().ml_auto().child(close));
        let mut model = self.model_hint(f.provider).unwrap_or_else(|| "Default model".into());
        match f.perm {
            Perm::Ask => {}
            Perm::AutoEdit => model.push_str(" · auto-edit"),
            Perm::Plan => model.push_str(" · plan only"),
        }
        let agent = chip("form-agent", f.picker == Some(Picker::Agent))
            .child(ui::dot(7., provider_color(f.provider)))
            .child(div().font_weight(FontWeight::SEMIBOLD).child(provider_name(f.provider)))
            .child(div().text_color(rgba(TEXT_3)).child(model))
            .child(icon("chevron-down", 12., TEXT_4))
            .capture_any_mouse_down(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                this.toggle_picker(Picker::Agent, cx);
            }));
        let base = f.branches.get(f.base).map(|(b, _)| b.clone()).unwrap_or_default();
        let branch = f.worktree.then(|| {
            chip("form-branch", f.picker == Some(Picker::Branch))
                .child(icon("branch", 14., TEXT_3))
                .child(div().font_family(MONO).text_size(px(12.5)).font_weight(FontWeight::MEDIUM).child(base.clone()))
                .child(icon("chevron-down", 12., TEXT_4))
                .capture_any_mouse_down(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    this.toggle_picker(Picker::Branch, cx);
                }))
        });
        let agent_menu = (f.picker == Some(Picker::Agent)).then(|| ui::dropdown(36., self.agent_picker(cx)));
        let branch_menu = (f.picker == Some(Picker::Branch)).then(|| ui::dropdown(36., self.branch_picker(cx)));
        let ready = self.session_ready(cx);
        let send = ui::primary(div().id("form-start").ml_auto().size(px(32.)).flex().flex_none().items_center().justify_center().rounded(px(16.)).cursor_pointer())
            .child(icon("arrow-up", 16., WHITE))
            .when(ready, |d| d.on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.start_session(window, cx))))
            .when(!ready, |d| d.opacity(0.5).cursor_default());
        let problem = if f.worktree { name_problem(&self.new_name(cx), &f.taken) } else { None };
        let name_field = f.worktree.then(|| {
            ui::field_box()
                .child(icon("worktree", 14., TEXT_3))
                .child(div().flex_1().min_w_0().font_family(MONO).child(Input::new(&f.name).appearance(false).p_0().text_size(px(13.))))
                .children(problem.map(|p| div().flex_none().text_size(px(12.)).text_color(rgba(FAILED)).child(p)))
        });
        let composer = div()
            .flex()
            .flex_col()
            .rounded(px(14.))
            .bg(rgba(WHITE))
            .shadow(vec![ui::ring(SEPARATOR_STRONG, 0.5), ui::shadow(0x1111130a, 1., 2.)])
            // The textarea pads itself 8px × 10px and wraps 10px short of its edge; the frame restores the design's 14/16/4 and its line breaks.
            .child(div().pt(px(6.)).pl(px(6.)).mr(px(-6.)).child(Textarea::new(&f.prompt).appearance(false).h(px(105.)).text_size(px(15.)).line_height(px(23.25))))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .pt(px(6.))
                    .px(px(10.))
                    .pb(px(10.))
                    .text_size(px(13.))
                    .child(div().relative().child(agent).children(agent_menu))
                    .children(branch.map(|b| div().relative().child(b).children(branch_menu)))
                    .child(send),
            );
        let mono = |s: String| div().font_family(MONO).text_color(rgba(TEXT_2)).child(s);
        let summary: Vec<AnyElement> = if f.worktree {
            vec![
                div().child("New branch").into_any_element(),
                mono(self.new_name(cx)).into_any_element(),
                div().child("from").into_any_element(),
                mono(base).into_any_element(),
            ]
        } else {
            let place = self.repo().map(|r| r.branch.clone()).or_else(|| self.cwd().map(|c| tilde(&c))).unwrap_or_default();
            vec![div().child("In").into_any_element(), mono(place).into_any_element()]
        };
        let footer = div()
            .flex()
            .items_center()
            .gap(px(6.))
            .px(px(6.))
            .text_size(px(12.))
            .text_color(rgba(TEXT_3))
            .whitespace_nowrap()
            .children(summary)
            .child(div().ml_auto().text_color(rgba(TEXT_4)).child("⌘↵ to start · esc to cancel"));
        div().absolute().top(px(110.)).left_0().right_0().flex().justify_center().child(
            // The design's 0.5px border renders 1px wide and insets the sheet's content.
            ui::pop(div().w(px(640.)).pt(px(17.)).px(px(17.)).pb(px(15.)).flex().flex_col().gap(px(10.))).occlude().child(header).children(name_field).child(composer).child(footer),
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
        self.store.add(&path);
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
            _ => "Add project",
        };
        let cancel = ui::large(ui::button("repo-cancel", Variant::Ghost, None, "Cancel").text_color(rgba(TEXT)))
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.close_overlay(window, cx)));
        let submit = ui::large(ui::button("repo-submit", Variant::Primary, None, label))
            .when(ready, |d| d.on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.save_repo(window, cx))))
            .when(!ready, |d| d.opacity(0.5).cursor_default());
        let close = ui::icon_button("repo-close", "x").on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.close_overlay(window, cx)));
        let title = if editing { "Project settings" } else { "Add project" };
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
    use super::{auto_name, name_problem, repo_from_url, slug};
    use std::collections::HashSet;

    #[test]
    fn slug_names_a_worktree_after_the_prompt() {
        assert_eq!(slug("The RestoreView snapshot fails on CI"), "the-restoreview-snapshot-fails");
        assert_eq!(slug("fix: flaky!"), "fix-flaky");
        assert_eq!(slug("  …  "), "");
    }

    #[test]
    fn an_empty_name_falls_back_to_a_free_one() {
        let taken: HashSet<String> = ["fix-flaky", "brave-otter", "Brave-Heron", "Fix-Login", "fix-login-2", "release/1.0"].map(String::from).into();
        assert_eq!(auto_name("fix: flaky!", 0, &taken), "fix-flaky-2");
        assert_eq!(auto_name("Fix login", 0, &taken), "fix-login-3");
        assert_eq!(auto_name("Release", 0, &taken), "release-2");
        assert_eq!(auto_name("Add dark mode", 0, &taken), "add-dark-mode");
        assert_eq!(auto_name("", 0, &taken), "brave-maple");
        assert_eq!(auto_name("", 63, &HashSet::new()), "swift-lynx");
        assert_eq!(auto_name("", 64, &HashSet::new()), "brave-otter");
    }

    #[test]
    fn a_name_must_be_free_and_valid_for_git() {
        let taken: HashSet<String> = ["main", "Foo", "fix/login"].map(String::from).into();
        assert_eq!(name_problem("fix-login_2.0", &taken), None);
        for used in ["main", "foo", "fix"] {
            assert_eq!(name_problem(used, &taken), Some("A worktree or branch with this name already exists"), "{used}");
        }
        for bad in ["", "fix login", "fix/login", "-x", ".x", "x.", "a..b", "x.lock", "tên", "HEAD"] {
            assert_eq!(name_problem(bad, &taken), Some("Use letters, digits, - _ or ."), "{bad}");
        }
    }

    #[test]
    fn repo_name_comes_from_the_url() {
        assert_eq!(repo_from_url("https://github.com/org/app-ios.git"), "app-ios");
        assert_eq!(repo_from_url("git@github.com:org/app-ios.git"), "app-ios");
        assert_eq!(repo_from_url("https://github.com/org/web/ "), "web");
    }
}
