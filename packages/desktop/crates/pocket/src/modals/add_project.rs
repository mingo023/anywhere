use crate::desktop::Desktop;
use crate::desktop::chrome::Overlay;
use crate::modals::form::{default_base, footer, home, typed_or};
use crate::util::{basename, tilde};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::path::{Path, PathBuf};
use store::RepoConfig;
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
            Source::Local => named && matches!(self.probe, Some(Probe::Git { .. })),
            Source::Clone => named && self.path.is_some() && !url.trim().is_empty(),
        }
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

fn field(label: &str, body: impl IntoElement) -> Div {
    div().flex_1().min_w_0().flex().flex_col().gap(px(6.)).child(ui::field_label(label.to_string())).child(body)
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
        (Self { url, name, setup, draft: RepoDraft::default() }, subs)
    }
}

impl Desktop {
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

    fn repo_form_name(&self, cx: &App) -> String {
        let f = &self.repo_form;
        typed_or(&f.name, || f.draft.fallback_name(&f.url.read(cx).value()), cx)
    }

    pub fn project_settings(&mut self, _: &crate::actions::ProjectSettings, window: &mut Window, cx: &mut Context<Self>) {
        self.close_menus();
        self.overlay = Some(Overlay::AddRepo);
        self.reset_repo_form(self.project.clone(), window, cx);
        cx.notify();
    }

    pub fn reset_repo_form(&mut self, editing: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        let cfg = editing.as_ref().and_then(|p| self.store.repos.get(p)).cloned().unwrap_or_default();
        let color = match &editing {
            Some(p) => self.repo_color(p),
            None => PALETTE[self.projects().len() % PALETTE.len()],
        };
        let f = &mut self.repo_form;
        f.draft = RepoDraft { color, worktrees: cfg.worktrees, copy: cfg.copy, editing: editing.clone(), ..RepoDraft::default() };
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
        f.draft.path = Some(path.clone());
        if f.draft.source == Source::Clone {
            return;
        }
        f.draft.probe = Some(Probe::Loading);
        f.name.update(cx, |s, cx| s.set_placeholder(basename(&path), window, cx));
        let dir = path.clone();
        let task = cx.background_executor().spawn(async move { (git::read(&dir), git::branches(&dir), git::remotes(&dir)) });
        cx.spawn(async move |this, cx| {
            let (repo, branches, remotes) = task.await;
            this.update(cx, |d, cx| {
                let f = &mut d.repo_form.draft;
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
        f.draft.ready(&self.repo_form_name(cx), &f.url.read(cx).value())
    }

    fn save_repo(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.repo_ready(cx) {
            return;
        }
        let f = &self.repo_form;
        let Some(path) = f.draft.path.clone() else { return };
        let name = self.repo_form_name(cx);
        let cfg = RepoConfig {
            name: name.clone(),
            color: f.draft.color,
            base: f.draft.branches.get(f.draft.base).cloned().unwrap_or_default(),
            worktrees: f.draft.worktrees.clone(),
            setup: f.setup.read(cx).value().trim().to_string(),
            copy: f.draft.copy.clone(),
        };
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

    fn add_repo(&mut self, path: String, cfg: RepoConfig, window: &mut Window, cx: &mut Context<Self>) {
        self.store.add(&path);
        self.store.repos.insert(path.clone(), cfg);
        self.store.save();
        self.close_overlay(window, cx);
        self.select_project(path, cx);
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
            None => (None, "Pick the folder of a git repository.".into(), TEXT_4),
            Some(Probe::Loading) => (None, "Checking…".into(), TEXT_4),
            Some(Probe::NotGit) => (Some("x"), "Not a git repository".into(), FAILED),
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
        let f = &self.repo_form;
        let source = div().id("repo-source").child(ui::segmented(
            vec![Segment { icon: Some("folder"), value: Source::Local, label: "Local folder".into(), badge: None }, Segment { icon: Some("external"), value: Source::Clone, label: "Clone from URL".into(), badge: None }],
            f.draft.source,
            false,
            true,
            |this: &mut Self, v, cx| {
                let f = &mut this.repo_form.draft;
                f.source = v;
                f.path = (v == Source::Clone).then(|| format!("{}/code", home())).filter(|p| Path::new(p).is_dir());
                f.probe = None;
                f.branches.clear();
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
        let folder = if f.draft.worktrees.is_empty() { format!("~/.worktrees/{name}") } else { tilde(&f.draft.worktrees) };
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
    use super::{Probe, RepoDraft, Source, repo_from_url};
    use std::path::PathBuf;

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
    fn a_local_folder_is_ready_once_named_and_found_to_be_a_git_repository() {
        let git = || Some(Probe::Git { branch: "main".into(), clean: true, remotes: 1 });
        assert!(local(git()).ready("app", ""));
        assert!(!local(git()).ready("", ""));
        assert!(!local(Some(Probe::Loading)).ready("app", ""));
        assert!(!local(Some(Probe::NotGit)).ready("app", ""));
        assert!(!local(None).ready("app", ""));
        assert!(!RepoDraft { busy: true, ..local(git()) }.ready("app", ""));
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
}
