use crate::desktop::Desktop;
use crate::desktop::chrome::{Confirm, Overlay, Screen};
use crate::modals::form::{default_base, footer, home, typed_or};
use crate::settings::dropdown::dropdown;
use crate::settings::models::{effort_label, effort_options, model_label, model_options};
use crate::util::{basename, tilde};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::path::{Path, PathBuf};
use store::prefs::agents::efforts;
use store::{LaunchPick, RepoConfig};
use theme::*;
use ui::{Segment, Variant};

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
    url: Entity<InputState>,
    name: Entity<InputState>,
    setup: Entity<InputState>,
    teardown: Entity<InputState>,
    dev_url: Entity<InputState>,
    draft: RepoDraft,
}

/// The form's choices besides its text inputs.
struct RepoDraft {
    source: Source,
    path: Option<String>,
    probe: Option<Probe>,
    color: u32,
    branches: Vec<String>,
    base: usize,
    worktrees: String,
    copy: Vec<String>,
    agent: LaunchPick,
    /// The base branch the repository starts on, which Reset goes back to.
    default_base: usize,
    /// Reset is asking to be confirmed.
    resetting: bool,
    editing: Option<String>,
    busy: bool,
    error: Option<String>,
}

impl Default for RepoDraft {
    fn default() -> Self {
        Self {
            source: Source::Local,
            path: None,
            probe: None,
            color: PALETTE[0],
            branches: Vec::new(),
            base: 0,
            worktrees: String::new(),
            copy: Vec::new(),
            agent: LaunchPick::default(),
            default_base: 0,
            resetting: false,
            editing: None,
            busy: false,
            error: None,
        }
    }
}

impl RepoDraft {
    fn fallback_name(&self, url: &str) -> String {
        match self.source {
            Source::Local => self.path.as_deref().map(basename).unwrap_or_default(),
            Source::Clone => repo_from_url(url),
        }
    }

    fn ready(&self, name: &str, url: &str) -> bool {
        let named = !name.is_empty();
        match self.source {
            _ if self.busy => false,
            Source::Local => named && matches!(self.probe, Some(Probe::Git { .. } | Probe::NotGit)),
            Source::Clone => named && self.path.is_some() && !url.trim().is_empty(),
        }
    }

    /// A local folder that isn't a repository, so it never gets worktrees.
    fn plain_folder(&self) -> bool {
        self.source == Source::Local && matches!(self.probe, Some(Probe::NotGit))
    }

    fn add_copies(&mut self, repo: &str, paths: Vec<PathBuf>) {
        for p in paths {
            let Ok(rel) = p.strip_prefix(repo) else { continue };
            let rel = rel.to_string_lossy().into_owned();
            if !self.copy.contains(&rel) {
                self.copy.push(rel);
            }
        }
    }
}

fn repo_from_url(url: &str) -> String {
    let tail = url.trim().trim_end_matches('/').rsplit(['/', ':']).next().unwrap_or_default();
    tail.strip_suffix(".git").unwrap_or(tail).to_string()
}

/// The project's agent with a model or effort chosen; one chosen under App default keeps that agent.
fn pinned(mut pick: LaunchPick, app: &str, set: impl FnOnce(&mut LaunchPick)) -> LaunchPick {
    set(&mut pick);
    if pick.provider.is_empty() && !(pick.model.is_empty() && pick.effort.is_empty()) {
        pick.provider = app.into();
    }
    pick
}

/// What Reset would put back, as (setting, now, default); name and colour are the project's own.
fn project_changes(cfg: &RepoConfig, default_base: &str) -> Vec<(&'static str, String, String)> {
    let or = |value: String, none: &str| if value.is_empty() { none.to_string() } else { value };
    let script = |s: &str| if s.is_empty() { "empty" } else { "edited" }.to_string();
    let copies = match cfg.copy.len() {
        0 => "none".to_string(),
        1 => "1 file".to_string(),
        n => format!("{n} files"),
    };
    let agent = if cfg.agent.provider.is_empty() { "App default".to_string() } else { theme::provider_name(&cfg.agent.provider).to_string() };
    let model = or(cfg.agent.model.is_empty().then(String::new).unwrap_or_else(|| model_label(&cfg.agent.model)), "App default");
    let effort = or(cfg.agent.effort.is_empty().then(String::new).unwrap_or_else(|| effort_label(&cfg.agent.effort)), "App default");
    [
        ("Default base branch", cfg.base.clone(), default_base.to_string()),
        ("Worktrees folder", or(tilde(&cfg.worktrees), "Default"), "Default".into()),
        ("When a worktree is created", script(&cfg.setup), script("")),
        ("When a worktree is deleted", script(&cfg.teardown), script("")),
        ("Copy into each worktree", copies, "none".into()),
        ("Agent", agent, "App default".into()),
        ("Model", model, "App default".into()),
        ("Effort", effort, "App default".into()),
        ("Dev server URL", or(cfg.dev_url.clone(), "empty"), "empty".into()),
    ]
    .into_iter()
    .filter(|(_, now, default)| now != default)
    .collect()
}

fn field(label: &str, body: impl IntoElement) -> Div {
    div().flex_1().min_w_0().flex().flex_col().gap(px(6.)).child(ui::field_label(label.to_string())).child(body)
}

impl RepoForm {
    pub fn new(window: &mut Window, cx: &mut Context<Desktop>) -> (Self, Vec<Subscription>) {
        let url = cx.new(|cx| InputState::new(window, cx).placeholder("https://github.com/org/repo.git"));
        let name = cx.new(|cx| InputState::new(window, cx));
        let setup = cx.new(|cx| InputState::new(window, cx).placeholder("pnpm install"));
        let teardown = cx.new(|cx| InputState::new(window, cx).placeholder("docker compose down"));
        let dev_url = cx.new(|cx| InputState::new(window, cx).placeholder("localhost:3000"));
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
        (Self { url, name, setup, teardown, dev_url, draft: RepoDraft::default() }, subs)
    }
}

impl Desktop {
    pub(crate) fn pick_path(&self, dirs: bool, window: &mut Window, cx: &mut Context<Self>, then: impl FnOnce(&mut Self, Vec<PathBuf>, &mut Window, &mut Context<Self>) + 'static) {
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

    fn repo_form_name(&self, cx: &App) -> String {
        let f = &self.repo_form;
        typed_or(&f.name, || f.draft.fallback_name(&f.url.read(cx).value()), cx)
    }

    pub fn project_settings(&mut self, _: &crate::actions::ProjectSettings, window: &mut Window, cx: &mut Context<Self>) {
        self.edit_project(self.project.clone(), window, cx);
    }

    pub(crate) fn edit_project(&mut self, project: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        self.close_menus();
        self.overlay = Some(Overlay::AddRepo);
        self.reset_repo_form(project, window, cx);
        cx.notify();
    }

    /// Opens Add project on Clone, into the clone folder.
    pub(crate) fn clone_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open(Overlay::AddRepo, window, cx);
        self.set_repo_source(Source::Clone);
        cx.notify();
    }

    fn set_repo_source(&mut self, v: Source) {
        let root = self.store.clone_root(&home());
        let f = &mut self.repo_form.draft;
        f.source = v;
        f.path = (v == Source::Clone).then_some(root).filter(|p| Path::new(p).is_dir());
        f.probe = None;
        f.branches.clear();
    }

    pub fn reset_repo_form(&mut self, editing: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        let cfg = editing.as_ref().and_then(|p| self.store.repos.get(p)).cloned().unwrap_or_default();
        let color = match &editing {
            Some(p) => self.repo_color(p),
            None => PALETTE[self.projects().len() % PALETTE.len()],
        };
        self.settings.menu = None;
        if editing.is_some() {
            self.load_models(cx);
        }
        let f = &mut self.repo_form;
        f.draft = RepoDraft { color, worktrees: cfg.worktrees, copy: cfg.copy, agent: cfg.agent, editing: editing.clone(), ..RepoDraft::default() };
        f.url.update(cx, |s, cx| s.set_value("", window, cx));
        f.name.update(cx, |s, cx| {
            s.set_value(cfg.name, window, cx);
            s.set_placeholder("", window, cx);
        });
        f.setup.update(cx, |s, cx| s.set_value(cfg.setup, window, cx));
        f.teardown.update(cx, |s, cx| s.set_value(cfg.teardown, window, cx));
        f.dev_url.update(cx, |s, cx| s.set_value(cfg.dev_url, window, cx));
        if let Some(p) = editing {
            self.set_repo_path(p, window, cx);
        }
    }

    fn set_repo_path(&mut self, path: String, window: &mut Window, cx: &mut Context<Self>) {
        let base = self.store.repos.get(&path).map(|r| r.base.clone()).unwrap_or_default();
        let f = &mut self.repo_form;
        f.draft.path = Some(path.clone());
        if f.draft.source == Source::Clone {
            return;
        }
        f.draft.probe = Some(Probe::Loading);
        f.name.update(cx, |s, cx| s.set_placeholder(basename(&path), window, cx));
        let dir = path.clone();
        let task = cx.background_executor().spawn(async move { (git::read(&dir, true), git::branches(&dir), git::remotes(&dir)) });
        cx.spawn(async move |this, cx| {
            let (repo, branches, remotes) = task.await;
            this.update(cx, |d, cx| {
                let f = &mut d.repo_form.draft;
                if f.path.as_ref() != Some(&path) {
                    return;
                }
                let current = repo.as_ref().map(|r| r.branch.as_str()).unwrap_or_default();
                f.base = default_base(branches.iter().map(String::as_str), &base, current);
                f.default_base = default_base(branches.iter().map(String::as_str), "", current);
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
        f.draft.ready(&self.repo_form_name(cx), &f.url.read(cx).value())
    }

    fn save_repo(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.repo_ready(cx) {
            return;
        }
        let f = &self.repo_form;
        let Some(path) = f.draft.path.clone() else { return };
        let name = self.repo_form_name(cx);
        let cfg = self.repo_config(&path, name.clone(), cx);
        if f.draft.source == Source::Local {
            return self.add_repo(path, cfg, window, cx);
        }
        let url = f.url.read(cx).value().trim().to_string();
        let dest = format!("{path}/{name}");
        self.repo_form.draft.busy = true;
        self.repo_form.draft.error = None;
        let task = cx.background_executor().spawn({
            let dest = dest.clone();
            async move { git::clone(&url, &dest) }
        });
        cx.spawn_in(window, async move |this, cx| {
            let res = task.await;
            this.update_in(cx, |d, window, cx| {
                d.repo_form.draft.busy = false;
                match res {
                    Ok(()) => d.add_repo(dest, cfg, window, cx),
                    Err(e) => d.repo_form.draft.error = Some(e),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// The project as the form now has it.
    fn repo_config(&self, path: &str, name: String, cx: &App) -> RepoConfig {
        let f = &self.repo_form;
        RepoConfig {
            name,
            color: f.draft.color,
            base: f.draft.branches.get(f.draft.base).cloned().unwrap_or_default(),
            worktrees: f.draft.worktrees.clone(),
            setup: f.setup.read(cx).value().trim().to_string(),
            teardown: f.teardown.read(cx).value().trim().to_string(),
            copy: f.draft.copy.clone(),
            launch: self.store.repos.get(path).map(|r| r.launch.clone()).unwrap_or_default(),
            agent: f.draft.agent.clone(),
            dev_url: f.dev_url.read(cx).value().trim().to_string(),
        }
    }

    /// Puts the form back on the app's defaults; Save keeps it.
    fn reset_repo(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let f = &mut self.repo_form;
        let d = &mut f.draft;
        (d.base, d.agent, d.resetting) = (d.default_base, LaunchPick::default(), false);
        d.worktrees.clear();
        d.copy.clear();
        for input in [&f.setup, &f.teardown, &f.dev_url] {
            input.update(cx, |s, cx| s.set_value("", window, cx));
        }
        cx.notify();
    }

    fn agent_fields(&self, cx: &mut Context<Self>) -> Div {
        let agent = self.repo_form.draft.agent.clone();
        let app = self.store.agents.default_agent();
        let provider = if agent.provider.is_empty() { app } else { agent.provider.as_str() };
        let trigger = |id: &'static str, label: String| {
            ui::field_box().id(id).cursor_pointer().child(div().flex_1().min_w_0().truncate().text_size(px(13.)).child(label)).child(icon("chevron-down", 12., TEXT_4))
        };
        let app_label = format!("App default ({})", theme::provider_name(app));
        let agents: Vec<(String, String)> = std::iter::once((String::new(), app_label.clone())).chain(LaunchPick::PROVIDERS.map(|p| (p.to_string(), theme::provider_name(p).to_string()))).collect();
        let shown = agents.iter().find(|(v, _)| *v == agent.provider).map_or(app_label, |(_, l)| l.clone());
        let pick_agent = dropdown("project-agent", trigger("project-agent", shown), agents, &agent.provider, self, |d, v, _| {
            let draft = &mut d.repo_form.draft;
            draft.agent = draft.agent.switched(&v);
        }, cx);
        let models = model_options(provider, self.codex_models(), &agent.model, "App default");
        let shown = models.iter().find(|(v, _)| *v == agent.model).map_or_else(|| model_label(&agent.model), |(_, l)| l.clone());
        let pick_model = dropdown("project-model", trigger("project-model", shown), models, &agent.model, self, move |d, v, _| {
            d.repo_form.draft.agent = pinned(d.repo_form.draft.agent.clone(), app, |p| p.model = v);
        }, cx);
        let pick_effort = (!efforts(provider).is_empty()).then(|| {
            let shown = if agent.effort.is_empty() { "App default".to_string() } else { effort_label(&agent.effort) };
            let pick = dropdown("project-effort", trigger("project-effort", shown), effort_options(provider, "App default"), &agent.effort, self, move |d, v, _| {
                d.repo_form.draft.agent = pinned(d.repo_form.draft.agent.clone(), app, |p| p.effort = v);
            }, cx);
            field("Effort", pick)
        });
        div().flex().gap(px(14.)).child(field("Agent", pick_agent).flex_none().w(px(210.))).child(field("Model", pick_model)).children(pick_effort)
    }

    /// The Reset line under an edited project, or the list it asks to confirm.
    fn reset_line(&self, path: &str, name: &str, cx: &mut Context<Self>) -> Div {
        let f = &self.repo_form.draft;
        if !f.resetting {
            let link = ui::link("repo-reset", format!("Reset {name} to defaults\u{2026}")).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.repo_form.draft.resetting = true;
                cx.notify();
            }));
            return div().flex().justify_end().text_size(px(12.5)).child(link);
        }
        let base = f.branches.get(f.default_base).cloned().unwrap_or_default();
        let changes = project_changes(&self.repo_config(path, name.into(), cx), &base);
        let lead = match changes.len() {
            0 => "Every setting here is already at its default.".to_string(),
            1 => "Only this project changes. 1 setting will go back:".to_string(),
            n => format!("Only this project changes. {n} settings will go back:"),
        };
        let lines = changes.into_iter().map(|(label, now, default)| {
            div().py(px(5.)).flex().justify_between().gap(px(12.)).border_t(px(0.5)).border_color(SEPARATOR).child(div().text_color(TEXT).child(label)).child(div().flex_none().text_color(TEXT_3).child(format!("{now} \u{2192} {default}")))
        });
        let cancel = ui::button("repo-reset-cancel", Variant::Secondary, None, "Cancel").on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
            this.repo_form.draft.resetting = false;
            cx.notify();
        }));
        let reset = ui::button("repo-reset-go", Variant::Primary, None, "Reset").on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.reset_repo(window, cx)));
        div()
            .p(px(12.))
            .flex()
            .flex_col()
            .gap(px(6.))
            .rounded(px(10.))
            .bg(FILL_1)
            .text_size(px(12.5))
            .child(div().font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child(format!("Reset {name} to defaults?")))
            .child(div().pb(px(4.)).text_color(TEXT_2).child(lead))
            .children(lines)
            .child(div().pt(px(4.)).flex().justify_end().gap(px(8.)).child(cancel).child(reset))
    }

    fn add_repo(&mut self, path: String, cfg: RepoConfig, window: &mut Window, cx: &mut Context<Self>) {
        self.store.add(&path);
        self.store.repos.insert(path.clone(), cfg);
        self.store.save();
        self.close_overlay(window, cx);
        // Edit… in Settings opens this form; saving stays there.
        if self.screen != Screen::Settings {
            self.select_project(path, cx);
        } else {
            self.load_dev_urls(window, cx);
        }
        self.refresh_git(cx);
    }

    fn folder_card(&self, cx: &mut Context<Self>) -> Div {
        let f = &self.repo_form.draft;
        let clone = f.source == Source::Clone;
        let title = match (&f.path, clone) {
            (Some(p), true) => format!("Clone into {}", tilde(p)),
            (Some(p), false) => tilde(p),
            (None, true) => "Choose where to clone".into(),
            (None, false) => "No folder chosen".into(),
        };
        let (mark, line, color) = match &f.probe {
            _ if clone => (None, "The repository is cloned into a folder named after it.".to_string(), TEXT_4),
            None => (None, "Pick a folder.".into(), TEXT_4),
            Some(Probe::Loading) => (None, "Checking…".into(), TEXT_4),
            Some(Probe::NotGit) => (None, "Folder · not a git repository".into(), TEXT_3),
            Some(Probe::Git { branch, clean, remotes }) => {
                let state = if *clean { "clean" } else { "uncommitted changes" };
                let remotes = match remotes {
                    1 => "1 remote".to_string(),
                    n => format!("{n} remotes"),
                };
                (Some("check"), format!("Git repository · {branch} · {state} · {remotes}"), SUCCESS_TEXT)
            }
        };
        div()
            .h(px(60.))
            .px(px(14.))
            .flex()
            .items_center()
            .gap(px(12.))
            .rounded(px(14.))
            .bg(SURFACE)
            .shadow(vec![ui::ring(SEPARATOR, 0.5)])
            .child(div().size(px(36.)).flex().flex_none().items_center().justify_center().rounded(px(10.)).bg(FILL_2).child(icon("folder", 16., TEXT_2)))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(div().truncate().font_family(MONO).text_size(px(13.5)).font_weight(FontWeight::SEMIBOLD).child(title))
                    .child(div().flex().items_center().gap(px(4.)).text_size(px(12.5)).text_color(color).children(mark.map(|m| icon(m, 12., color))).child(line)),
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
        let agent = self.agent_fields(cx);
        let reset = self.repo_form.draft.editing.clone().map(|p| self.reset_line(&p, &name, cx));
        let f = &self.repo_form;
        let source = div().id("repo-source").child(ui::segmented(
            vec![Segment { icon: Some("folder"), value: Source::Local, label: "Local folder".into(), badge: None }, Segment { icon: Some("external"), value: Source::Clone, label: "Clone from URL".into(), badge: None }],
            f.draft.source,
            false,
            true,
            |this: &mut Self, v, cx| {
                this.set_repo_source(v);
                cx.notify();
            },
            cx,
        ));
        let url = (f.draft.source == Source::Clone).then(|| {
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
                .when(c == f.draft.color, |d| d.shadow(vec![ui::ring(rgba(c), 1.5)]))
                .child(ui::swatch(c, 22., 7.))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.repo_form.draft.color = c;
                    cx.notify();
                }))
        }));
        let identity = div()
            .flex()
            .gap(px(14.))
            .child(field("Name", ui::field_box().child(div().flex_1().child(Input::new(&f.name).appearance(false).p_0().text_size(px(14.))))))
            .child(field("Colour", colors));
        let count = f.draft.branches.len();
        let base_name = f.draft.branches.get(f.draft.base).cloned().unwrap_or_else(|| "Default branch".into());
        let base = ui::field_box()
            .id("repo-base")
            .when(count > 1, |d| d.cursor_pointer())
            .child(icon("branch", 13., TEXT_3))
            .child(div().flex_1().min_w_0().truncate().font_family(MONO).text_size(px(13.)).child(base_name))
            .child(icon("chevron-down", 12., TEXT_4))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.repo_form.draft.base = (this.repo_form.draft.base + 1) % count.max(1);
                cx.notify();
            }));
        let root = tilde(&self.store.worktree_root(&home()));
        let folder = if f.draft.worktrees.is_empty() { format!("{root}/{name}") } else { tilde(&f.draft.worktrees) };
        let worktrees = ui::field_box()
            .child(icon("folder", 13., TEXT_3))
            .child(div().flex_1().min_w_0().truncate().font_family(MONO).text_size(px(13.)).child(folder))
            .child(ui::link("repo-worktrees", "Change").on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                this.pick_path(true, window, cx, |d, paths, _, _| {
                    if let Some(p) = paths.into_iter().next() {
                        d.repo_form.draft.worktrees = p.to_string_lossy().into_owned();
                    }
                })
            })));
        let layout = div()
            .flex()
            .gap(px(14.))
            .child(field("Default base branch", base).child(div().text_size(px(12.)).text_color(TEXT_4).child("New worktrees branch off this unless you pick another.")))
            .child(field("Worktrees folder", worktrees));
        let setup = field(
            "When a worktree is created",
            ui::field_box().child(icon("terminal", 13., TEXT_3)).child(div().flex_1().font_family(MONO).child(Input::new(&f.setup).appearance(false).p_0().text_size(px(13.)))),
        );
        let teardown = field(
            "When a worktree is deleted",
            ui::field_box().child(icon("terminal", 13., TEXT_3)).child(div().flex_1().font_family(MONO).child(Input::new(&f.teardown).appearance(false).p_0().text_size(px(13.)))),
        );
        let dev_url = field(
            "Dev server URL",
            ui::field_box().child(icon("globe", 13., TEXT_3)).child(div().flex_1().font_family(MONO).child(Input::new(&f.dev_url).appearance(false).p_0().text_size(px(13.)))),
        )
        .child(div().text_size(px(12.)).text_color(TEXT_4).child("Opens in the in-app browser for this project."));
        let remove = f.draft.editing.clone().map(|p| {
            let text = div().flex_1().min_w_0().flex().flex_col().gap(px(2.)).child(ui::field_label("Remove project".to_string())).child(
                div().text_size(px(12.)).text_color(TEXT_4).child("Removes it from Anywhere. Files and worktrees stay on disk."),
            );
            let button = ui::button("repo-remove", Variant::Secondary, None, "Remove project\u{2026}").text_color(FAILED).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                (this.confirm, this.overlay) = (Some(Confirm::RemoveProject(p.clone())), Some(Overlay::Confirm));
                cx.notify();
            }));
            div().flex().items_center().gap(px(12.)).child(text).child(button)
        });
        let repo = f.draft.path.clone().filter(|_| f.draft.source == Source::Local);
        let copies = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(6.))
            .child(div().mr(px(2.)).text_size(px(12.5)).text_color(TEXT_3).child("Copy into each worktree"))
            .children(f.draft.copy.iter().enumerate().map(|(i, rel)| {
                ui::tag(rel.clone())
                    .id(("repo-copy", i))
                    .gap(px(4.))
                    .cursor_pointer()
                    .font_family(MONO)
                    .text_color(TEXT)
                    .child(icon("x", 9., TEXT_4))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.repo_form.draft.copy.remove(i);
                        cx.notify();
                    }))
            }))
            .when_some(repo, |d, repo| {
                d.child(ui::link("repo-add-file", "+ Add file").on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    let repo = repo.clone();
                    this.pick_path(false, window, cx, move |d, paths, _, _| d.repo_form.draft.add_copies(&repo, paths))
                })))
            });
        let editing = f.draft.editing.is_some();
        let note = match &f.draft.error {
            Some(e) => div().truncate().text_color(FAILED).child(e.clone()),
            None if editing || name.is_empty() => div(),
            None => div().truncate().child(format!("Adds {name} to the project rail")),
        };
        let ready = self.repo_ready(cx);
        let label = match () {
            _ if f.draft.busy => "Cloning…",
            _ if editing => "Save",
            _ => "Add project",
        };
        let cancel = ui::large(ui::button("repo-cancel", Variant::Ghost, None, "Cancel").text_color(TEXT))
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
        body.extend([self.folder_card(cx).into_any_element(), identity.into_any_element()]);
        if !f.draft.plain_folder() {
            body.extend([layout.into_any_element(), div().flex().flex_col().gap(px(10.)).child(setup).child(teardown).child(copies).into_any_element()]);
        }
        body.push(agent.into_any_element());
        body.push(dev_url.into_any_element());
        body.extend(remove.map(IntoElement::into_any_element));
        body.extend(reset.map(IntoElement::into_any_element));
        body.push(footer(note, cancel, submit).into_any_element());
        ui::modal(title, 640., 64., close, body)
    }
}

#[cfg(test)]
mod tests {
    use super::{Probe, RepoDraft, Source, pinned, project_changes, repo_from_url};
    use std::path::PathBuf;
    use store::{LaunchPick, RepoConfig};

    #[test]
    fn repo_name_comes_from_the_url() {
        assert_eq!(repo_from_url("https://github.com/org/app-ios.git"), "app-ios");
        assert_eq!(repo_from_url("git@github.com:org/app-ios.git"), "app-ios");
        assert_eq!(repo_from_url("https://github.com/org/web/ "), "web");
    }

    fn local(probe: Option<Probe>) -> RepoDraft {
        RepoDraft { path: Some("/src/app".into()), probe, ..RepoDraft::default() }
    }

    #[test]
    fn a_local_folder_is_ready_once_named_and_probed_whether_or_not_it_is_a_git_repository() {
        let git = || Some(Probe::Git { branch: "main".into(), clean: true, remotes: 1 });
        assert!(local(git()).ready("app", ""));
        assert!(local(Some(Probe::NotGit)).ready("app", ""));
        assert!(!local(git()).ready("", ""));
        assert!(!local(Some(Probe::Loading)).ready("app", ""));
        assert!(!local(None).ready("app", ""));
        assert!(!RepoDraft { busy: true, ..local(git()) }.ready("app", ""));
    }

    #[test]
    fn only_a_local_folder_found_not_to_be_a_repository_is_plain() {
        assert!(local(Some(Probe::NotGit)).plain_folder());
        assert!(!local(Some(Probe::Git { branch: "main".into(), clean: true, remotes: 1 })).plain_folder());
        assert!(!local(Some(Probe::Loading)).plain_folder());
        assert!(!RepoDraft { source: Source::Clone, ..local(Some(Probe::NotGit)) }.plain_folder());
    }

    #[test]
    fn a_clone_is_ready_once_named_with_a_url_and_a_destination_while_not_already_cloning() {
        let clone = RepoDraft { source: Source::Clone, path: Some("/code".into()), ..RepoDraft::default() };
        let url = "https://github.com/org/app.git";
        assert!(clone.ready("app", url));
        assert!(!clone.ready("", url));
        assert!(!clone.ready("app", "  "));
        assert!(!RepoDraft { path: None, ..clone }.ready("app", url));
        let clone = RepoDraft { source: Source::Clone, path: Some("/code".into()), busy: true, ..RepoDraft::default() };
        assert!(!clone.ready("app", url));
    }

    #[test]
    fn an_unnamed_project_takes_its_folder_name_or_the_repository_in_its_url() {
        let url = "git@github.com:org/web.git";
        assert_eq!(local(None).fallback_name(url), "app");
        assert_eq!(RepoDraft::default().fallback_name(url), "");
        let clone = RepoDraft { source: Source::Clone, path: Some("/code".into()), ..RepoDraft::default() };
        assert_eq!(clone.fallback_name(url), "web");
    }

    #[test]
    fn picked_files_are_copied_by_their_repository_path_once_and_outside_files_are_skipped() {
        let mut draft = RepoDraft { copy: vec![".env".into()], ..RepoDraft::default() };
        let picked = ["/src/app/.env", "/src/app/config/local.toml", "/elsewhere/key", "/src/app/config/local.toml"].map(PathBuf::from);
        draft.add_copies("/src/app", picked.into());
        assert_eq!(draft.copy, [".env", "config/local.toml"]);
    }

    #[test]
    fn a_model_chosen_under_app_default_keeps_the_agent_it_was_chosen_for() {
        let pick = pinned(LaunchPick::default(), "codex", |p| p.model = "gpt-6-sol".into());
        assert_eq!((pick.provider.as_str(), pick.model.as_str()), ("codex", "gpt-6-sol"));
        assert_eq!(pinned(LaunchPick::default(), "claude", |p| p.model.clear()), LaunchPick::default());
        let own = LaunchPick { provider: "claude".into(), ..LaunchPick::default() };
        assert_eq!(pinned(own, "codex", |p| p.effort = "high".into()).provider, "claude");
    }

    #[test]
    fn reset_lists_what_differs_from_the_app_s_defaults_but_not_the_name_or_colour() {
        let cfg = RepoConfig {
            name: "app".into(),
            color: 1,
            base: "develop".into(),
            setup: "pnpm install".into(),
            agent: LaunchPick { provider: "claude".into(), model: "opus".into(), effort: String::new(), access: String::new() },
            ..RepoConfig::default()
        };
        let changes = project_changes(&cfg, "main");
        let lines: Vec<_> = changes.iter().map(|(l, now, d)| format!("{l}: {now} > {d}")).collect();
        assert_eq!(lines, ["Default base branch: develop > main", "When a worktree is created: edited > empty", "Agent: Claude Code > App default", "Model: Opus > App default"]);
        assert!(project_changes(&RepoConfig { base: "main".into(), ..RepoConfig::default() }, "main").is_empty());
    }
}
