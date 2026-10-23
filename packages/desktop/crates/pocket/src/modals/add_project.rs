pub(crate) mod accounts;
pub(crate) mod host_folder;
pub(crate) mod icon;

use crate::desktop::Desktop;
use crate::desktop::chrome::{Confirm, Overlay, Screen};
use crate::modals::form::{footer, typed_or};
use crate::util::{basename, tilde};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use host_folder::Browse;
use std::collections::BTreeMap;
use std::path::PathBuf;
use store::{ProjectIcon, RepoConfig};
use theme::*;
use ui::Variant;

#[derive(Clone, Copy, PartialEq)]
pub enum Source {
    Local,
    Host,
}

enum Probe {
    Loading,
    Git { branch: String, clean: bool, remotes: usize },
    NotGit,
}

pub struct RepoForm {
    name: Entity<InputState>,
    setup: Entity<InputState>,
    teardown: Entity<InputState>,
    host_path: Entity<InputState>,
    host_scroll: UniformListScrollHandle,
    draft: RepoDraft,
}

/// The form's choices besides its text inputs.
struct RepoDraft {
    source: Source,
    path: Option<String>,
    probe: Option<Probe>,
    color: u32,
    icon: Option<ProjectIcon>,
    /// The account by provider its new sessions start on.
    accounts: BTreeMap<String, String>,
    /// The provider whose account menu is open.
    account_menu: Option<&'static str>,
    /// Reset is asking to be confirmed.
    resetting: bool,
    editing: Option<String>,
    browse: Option<Browse>,
}

impl Default for RepoDraft {
    fn default() -> Self {
        Self {
            source: Source::Local,
            path: None,
            probe: None,
            color: PALETTE[0],
            icon: None,
            accounts: BTreeMap::new(),
            account_menu: None,
            resetting: false,
            editing: None,
            browse: None,
        }
    }
}

impl RepoDraft {
    fn fallback_name(&self) -> String {
        self.path.as_deref().map(basename).unwrap_or_default()
    }

    fn ready(&self, name: &str) -> bool {
        !name.is_empty() && matches!(self.probe, Some(Probe::Git { .. } | Probe::NotGit))
    }

    /// A folder that isn't a repository, so it never gets worktrees.
    fn plain_folder(&self) -> bool {
        matches!(self.probe, Some(Probe::NotGit))
    }
}

/// What Reset would put back, as (setting, now, default); name, colour and icon are the project's own.
fn project_changes(cfg: &RepoConfig) -> Vec<(&'static str, String, String)> {
    let script = |s: &str| if s.is_empty() { "empty" } else { "edited" }.to_string();
    let accounts = |n: usize| if n == 0 { "default".to_string() } else { "picked".to_string() };
    [
        ("Accounts", accounts(cfg.accounts.len()), accounts(0)),
        ("When a worktree is created", script(&cfg.setup), script("")),
        ("When a worktree is deleted", script(&cfg.teardown), script("")),
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
        let name = cx.new(|cx| InputState::new(window, cx));
        let setup = cx.new(|cx| InputState::new(window, cx).placeholder("pnpm install"));
        let teardown = cx.new(|cx| InputState::new(window, cx).placeholder("docker compose down"));
        let host_path = cx.new(|cx| InputState::new(window, cx).placeholder("/home/me/code/app"));
        let subs = vec![
            cx.subscribe_in(&host_path, window, |this, field, ev: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { .. } = ev {
                    let typed = field.read(cx).value().trim().to_string();
                    this.browse_to(typed, window, cx);
                }
            }),
            cx.subscribe(&name, |_, _, _: &InputEvent, cx| cx.notify()),
        ];
        (Self { name, setup, teardown, host_path, host_scroll: UniformListScrollHandle::new(), draft: RepoDraft::default() }, subs)
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
        typed_or(&f.name, || f.draft.fallback_name(), cx)
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

    /// Adds the folder picked in Finder as a project, with the settings it had if it was one before.
    pub(crate) fn add_local_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_menus();
        self.pick_path(true, window, cx, |d, paths, window, cx| {
            let Some(path) = paths.into_iter().next().map(|p| p.to_string_lossy().into_owned()) else { return };
            let cfg = d.new_repo_config(&path);
            d.add_repo(path, cfg, window, cx);
        });
    }

    /// The settings `path` had if it was a project before, else the defaults in the next colour.
    pub(crate) fn new_repo_config(&self, path: &str) -> RepoConfig {
        self.store.repos.get(path).cloned().unwrap_or(RepoConfig { color: self.next_color(), ..RepoConfig::default() })
    }

    fn next_color(&self) -> u32 {
        PALETTE[self.projects().len() % PALETTE.len()]
    }

    /// Opens Add project on Folder on a host.
    pub(crate) fn open_host_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open(Overlay::AddRepo, window, cx);
        self.set_repo_source(Source::Host);
        cx.notify();
    }

    pub(crate) fn set_repo_source(&mut self, v: Source) {
        let f = &mut self.repo_form.draft;
        f.source = v;
        f.path = None;
        f.probe = None;
        f.browse = None;
    }

    pub fn reset_repo_form(&mut self, editing: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        let cfg = editing.as_ref().and_then(|p| self.store.repos.get(p)).cloned().unwrap_or_default();
        let color = match &editing {
            Some(p) => self.repo_color(p),
            None => self.next_color(),
        };
        self.settings.menu = None;
        let f = &mut self.repo_form;
        f.draft = RepoDraft { color, icon: cfg.icon, accounts: cfg.accounts, editing: editing.clone(), ..RepoDraft::default() };
        f.name.update(cx, |s, cx| {
            s.set_value(cfg.name, window, cx);
            s.set_placeholder("", window, cx);
        });
        f.setup.update(cx, |s, cx| s.set_value(cfg.setup, window, cx));
        f.teardown.update(cx, |s, cx| s.set_value(cfg.teardown, window, cx));
        if let Some(p) = editing {
            self.set_repo_path(p, window, cx);
        }
    }

    fn set_repo_path(&mut self, path: String, window: &mut Window, cx: &mut Context<Self>) {
        let f = &mut self.repo_form;
        f.draft.path = Some(path.clone());
        f.draft.probe = Some(Probe::Loading);
        f.name.update(cx, |s, cx| s.set_placeholder(basename(&path), window, cx));
        let dir = path.clone();
        let task = cx.background_executor().spawn(async move {
            let runner = hosts::runner_for(&dir);
            (git::read(&runner, &dir, true), git::remotes(&runner, &dir))
        });
        cx.spawn(async move |this, cx| {
            let (repo, remotes) = task.await;
            this.update(cx, |d, cx| {
                let f = &mut d.repo_form.draft;
                if f.path.as_ref() != Some(&path) {
                    return;
                }
                f.probe = Some(match repo {
                    Some(r) => Probe::Git { clean: r.files.is_empty(), branch: r.branch, remotes },
                    None => Probe::NotGit,
                });
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn repo_ready(&self, cx: &App) -> bool {
        self.repo_form.draft.ready(&self.repo_form_name(cx))
    }

    pub(crate) fn save_repo(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.repo_ready(cx) {
            return;
        }
        let Some(path) = self.repo_form.draft.path.clone() else { return };
        let cfg = self.repo_config(&path, self.repo_form_name(cx), cx);
        self.add_repo(path, cfg, window, cx);
    }

    /// The project as the form now has it; its dev server URL is set in Settings › Browser.
    fn repo_config(&self, path: &str, name: String, cx: &App) -> RepoConfig {
        let f = &self.repo_form;
        let saved = self.store.repos.get(path);
        RepoConfig {
            name,
            color: f.draft.color,
            icon: f.draft.icon.clone(),
            setup: f.setup.read(cx).value().trim().to_string(),
            teardown: f.teardown.read(cx).value().trim().to_string(),
            launch: saved.map(|r| r.launch.clone()).unwrap_or_default(),
            dev_url: saved.map(|r| r.dev_url.clone()).unwrap_or_default(),
            gh_account: saved.map(|r| r.gh_account.clone()).unwrap_or_default(),
            accounts: f.draft.accounts.clone(),
        }
    }

    /// Puts the form back on the app's defaults; Save keeps it.
    fn reset_repo(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let f = &mut self.repo_form;
        f.draft.resetting = false;
        f.draft.accounts.clear();
        for input in [&f.setup, &f.teardown] {
            input.update(cx, |s, cx| s.set_value("", window, cx));
        }
        cx.notify();
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
        let changes = project_changes(&self.repo_config(path, name.into(), cx));
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

    pub(crate) fn add_repo(&mut self, path: String, cfg: RepoConfig, window: &mut Window, cx: &mut Context<Self>) {
        self.store.add(&path);
        self.store.repos.insert(path.clone(), cfg);
        self.store.save();
        if let (Some(host), _) = hosts::split(&path) {
            self.sync_host_registry(host, cx);
        }
        // A clone can finish after its dialog was dismissed and another opened.
        if matches!(self.overlay, Some(Overlay::AddRepo | Overlay::Clone)) {
            self.close_overlay(window, cx);
        }
        // Edit… in Settings opens this form; saving stays there.
        if self.screen != Screen::Settings {
            self.select_project(path, cx);
        } else {
            self.load_dev_urls(window, cx);
        }
        self.refresh_git(cx);
    }

    fn folder_card(&self) -> Div {
        let f = &self.repo_form.draft;
        let title = match &f.path {
            Some(p) => match hosts::split(p) {
                (Some(h), path) => format!("{}:{path}", self.host_name(h)),
                (None, _) => tilde(p),
            },
            None => "No folder chosen".into(),
        };
        let (mark, line, color) = match &f.probe {
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
    }

    pub fn repo_view(&mut self, _: &mut Window, cx: &mut Context<Self>) -> Div {
        let name = self.repo_form_name(cx);
        let reset = self.repo_form.draft.editing.clone().map(|p| self.reset_line(&p, &name, cx));
        let accounts = self.project_accounts_field(cx);
        let f = &self.repo_form;
        let identity = div()
            .flex()
            .gap(px(14.))
            .child(field("Name", ui::field_box().child(div().flex_1().child(Input::new(&f.name).appearance(false).p_0().text_size(px(14.))))))
            .child(field("Icon", self.icon_choices(&name, cx)).flex_none());
        let setup = field(
            "When a worktree is created",
            ui::field_box().child(icon("terminal", 13., TEXT_3)).child(div().flex_1().font_family(MONO).child(Input::new(&f.setup).appearance(false).p_0().text_size(px(13.)))),
        );
        let teardown = field(
            "When a worktree is deleted",
            ui::field_box().child(icon("terminal", 13., TEXT_3)).child(div().flex_1().font_family(MONO).child(Input::new(&f.teardown).appearance(false).p_0().text_size(px(13.)))),
        );
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
        let editing = f.draft.editing.is_some();
        let note = if editing || name.is_empty() { div() } else { div().truncate().child(format!("Adds {name} to the project rail")) };
        let ready = self.repo_ready(cx);
        let label = if editing { "Save" } else { "Add project" };
        let cancel = ui::large(ui::button("repo-cancel", Variant::Ghost, None, "Cancel").text_color(TEXT))
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.close_overlay(window, cx)));
        let submit = ui::large(ui::button("repo-submit", Variant::Primary, None, label))
            .when(ready, |d| d.on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.save_repo(window, cx))))
            .when(!ready, |d| d.opacity(0.5).cursor_default());
        let close = ui::icon_button("repo-close", "x").on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.close_overlay(window, cx)));
        let title = if editing { "Project settings" } else { "Add project" };
        let mut body: Vec<AnyElement> = Vec::new();
        if f.draft.source == Source::Host && !editing {
            body.push(self.host_browser(cx).into_any_element());
        }
        body.extend([self.folder_card().into_any_element(), identity.into_any_element()]);
        body.extend(accounts.map(IntoElement::into_any_element));
        if !f.draft.plain_folder() {
            body.push(div().flex().flex_col().gap(px(10.)).child(setup).child(teardown).into_any_element());
        }
        body.extend(remove.map(IntoElement::into_any_element));
        body.extend(reset.map(IntoElement::into_any_element));
        body.push(footer(note, cancel, submit).into_any_element());
        ui::modal(title, 640., 64., close, body)
    }
}

#[cfg(test)]
mod tests {
    use super::{Probe, RepoDraft, project_changes};
    use store::{ProjectIcon, RepoConfig};

    fn local(probe: Option<Probe>) -> RepoDraft {
        RepoDraft { path: Some("/src/app".into()), probe, ..RepoDraft::default() }
    }

    #[test]
    fn a_local_folder_is_ready_once_named_and_probed_whether_or_not_it_is_a_git_repository() {
        let git = || Some(Probe::Git { branch: "main".into(), clean: true, remotes: 1 });
        assert!(local(git()).ready("app"));
        assert!(local(Some(Probe::NotGit)).ready("app"));
        assert!(!local(git()).ready(""));
        assert!(!local(Some(Probe::Loading)).ready("app"));
        assert!(!local(None).ready("app"));
    }

    #[test]
    fn only_a_folder_found_not_to_be_a_repository_is_plain() {
        assert!(local(Some(Probe::NotGit)).plain_folder());
        assert!(!local(Some(Probe::Git { branch: "main".into(), clean: true, remotes: 1 })).plain_folder());
        assert!(!local(Some(Probe::Loading)).plain_folder());
    }

    #[test]
    fn an_unnamed_project_takes_its_folder_name() {
        assert_eq!(local(None).fallback_name(), "app");
        assert_eq!(RepoDraft::default().fallback_name(), "");
    }

    #[test]
    fn reset_puts_back_the_accounts_a_project_picks() {
        let cfg = RepoConfig { accounts: [("claude".to_string(), "work".to_string())].into(), ..RepoConfig::default() };
        let lines: Vec<_> = project_changes(&cfg).iter().map(|(l, now, d)| format!("{l}: {now} > {d}")).collect();
        assert_eq!(lines, ["Accounts: picked > default"]);
    }

    #[test]
    fn reset_lists_the_scripts_set_but_not_the_name_colour_icon_or_dev_server() {
        let cfg = RepoConfig {
            name: "app".into(),
            color: 1,
            icon: Some(ProjectIcon::Emoji("🚀".into())),
            dev_url: "localhost:3000".into(),
            setup: "pnpm install".into(),
            ..RepoConfig::default()
        };
        let lines: Vec<_> = project_changes(&cfg).iter().map(|(l, now, d)| format!("{l}: {now} > {d}")).collect();
        assert_eq!(lines, ["When a worktree is created: edited > empty"]);
        assert!(project_changes(&RepoConfig::default()).is_empty());
    }
}
