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
use std::time::Duration;
use store::{ProjectIcon, RepoConfig};
use theme::*;
use ui::Variant;

const SHEET_TOP: f32 = 48.;

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
    /// The edited project as the form opened on it, which Save is greyed out matching.
    saved: Option<RepoConfig>,
    copied: bool,
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
            saved: None,
            copied: false,
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

    /// Whether `now` differs from the project as the form opened on it; a new project always does.
    fn unsaved(&self, now: &RepoConfig) -> bool {
        self.saved.as_ref() != Some(now)
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

#[derive(Clone, Copy, Debug, PartialEq)]
enum Tone {
    Plain,
    Dim,
    Good,
    Dirty,
}

/// One chip under the chosen folder's path.
#[derive(Debug, PartialEq)]
struct Fact {
    glyph: Option<&'static str>,
    label: String,
    tone: Tone,
}

impl Fact {
    fn new(glyph: Option<&'static str>, label: impl Into<String>, tone: Tone) -> Self {
        Self { glyph, label: label.into(), tone }
    }
}

/// What the chosen folder is, as the chips under its path read.
fn folder_facts(probe: Option<&Probe>) -> Vec<Fact> {
    match probe {
        None => vec![Fact::new(None, "Pick a folder", Tone::Dim)],
        Some(Probe::Loading) => vec![Fact::new(None, "Checking\u{2026}", Tone::Dim)],
        Some(Probe::NotGit) => vec![Fact::new(Some("folder"), "Not a git repository", Tone::Plain)],
        Some(Probe::Git { branch, clean, remotes }) => {
            let state = if *clean { Fact::new(Some("check"), "Clean", Tone::Good) } else { Fact::new(None, "Uncommitted changes", Tone::Dirty) };
            let remotes = match remotes {
                0 => "No remote".to_string(),
                1 => "1 remote".to_string(),
                n => format!("{n} remotes"),
            };
            vec![Fact::new(Some("branch"), branch.clone(), Tone::Plain), state, Fact::new(Some("cloud"), remotes, Tone::Plain)]
        }
    }
}

fn fact_chip(Fact { glyph, label, tone }: Fact) -> Div {
    let color = match tone {
        Tone::Plain | Tone::Dirty => TEXT_2,
        Tone::Dim => TEXT_4,
        Tone::Good => SUCCESS_TEXT,
    };
    div()
        .h(px(22.))
        .px(px(8.))
        .flex()
        .flex_none()
        .items_center()
        .gap(px(5.))
        .rounded(px(11.))
        .bg(FILL_2)
        .shadow(vec![ui::ring(SEPARATOR, 0.5)])
        .text_size(px(11.5))
        .text_color(color)
        .when(tone == Tone::Dirty, |d| d.child(ui::dot(6., WAITING_DOT)))
        .children(glyph.map(|g| icon(g, 11., color)))
        .child(label)
}

/// A row on a `group` plate.
fn plate_row() -> Div {
    div().h(px(46.)).px(px(12.)).flex().items_center().gap(px(10.))
}

/// Rows on one rounded plate, split by hairlines, with an optional heading above and note below.
fn group(heading: Option<&str>, rows: Vec<AnyElement>, note: Option<&str>) -> Div {
    let rows = rows.into_iter().enumerate().map(|(i, r)| div().when(i > 0, |d| d.border_t(px(0.5)).border_color(SEPARATOR)).child(r));
    div()
        .flex()
        .flex_col()
        .children(heading.map(|h| div().px(px(12.)).pb(px(7.)).text_size(px(12.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT_3).child(h.to_string())))
        .child(div().flex().flex_col().rounded(px(12.)).bg(FILL_1).shadow(vec![ui::ring(SEPARATOR, 0.5)]).children(rows))
        .children(note.map(|n| div().px(px(12.)).pt(px(7.)).text_size(px(12.)).text_color(TEXT_4).child(n.to_string())))
}

fn script_row(label: &str, hint: &str, input: &Entity<InputState>) -> AnyElement {
    let head = div()
        .flex()
        .items_center()
        .justify_between()
        .child(div().text_size(px(13.)).text_color(TEXT).child(label.to_string()))
        .child(div().text_size(px(12.)).text_color(TEXT_4).child(hint.to_string()));
    let field = ui::field_box()
        .h(px(36.))
        .rounded(px(9.))
        .font_family(MONO)
        .child(div().text_size(px(13.)).text_color(TEXT_4).child("$"))
        .child(div().flex_1().child(Input::new(input).appearance(false).p_0().text_size(px(13.))));
    div().p(px(12.)).flex().flex_col().gap(px(8.)).child(head).child(field).into_any_element()
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
            self.set_repo_path(p.clone(), window, cx);
            self.repo_form.draft.saved = Some(self.repo_config(&p, self.repo_form_name(cx), cx));
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

    fn repo_unsaved(&self, cx: &App) -> bool {
        let f = &self.repo_form.draft;
        f.editing.as_ref().is_none_or(|path| f.unsaved(&self.repo_config(path, self.repo_form_name(cx), cx)))
    }

    fn copy_repo_path(&mut self, path: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(path));
        // A second copy inside the window would have the first timer clear the check early.
        if self.repo_form.draft.copied {
            return;
        }
        self.repo_form.draft.copied = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(1200)).await;
            this.update(cx, |d, cx| {
                d.repo_form.draft.copied = false;
                cx.notify();
            })
            .ok();
        })
        .detach();
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

    /// Reset and Remove, on their own plate under an edited project's settings.
    fn project_actions(&self, path: &str, name: &str, cx: &mut Context<Self>) -> Div {
        let changed = match project_changes(&self.repo_config(path, name.into(), cx)).len() {
            0 => "At defaults".to_string(),
            n => format!("{n} changed"),
        };
        let row = |id: &'static str, glyph: &str, label: &str, color: Token, aside: String| {
            plate_row()
                .id(id)
                .cursor_pointer()
                .hover(|d| d.bg(FILL_1))
                .child(icon(glyph, 14., color))
                .child(div().flex_1().text_size(px(13.)).text_color(color).child(label.to_string()))
                .child(div().text_size(px(12.)).text_color(TEXT_4).child(aside))
        };
        let reset = row("repo-reset", "discard", "Reset to defaults\u{2026}", TEXT, changed).rounded_t(px(12.)).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
            this.repo_form.draft.resetting ^= true;
            cx.notify();
        }));
        let p = path.to_string();
        let remove = row("repo-remove", "trash", "Remove from Anywhere\u{2026}", FAILED, "Files and worktrees stay on disk".into()).rounded_b(px(12.)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
            (this.confirm, this.overlay) = (Some(Confirm::RemoveProject(p.clone())), Some(Overlay::Confirm));
            cx.notify();
        }));
        let mut rows = vec![reset.into_any_element()];
        if self.repo_form.draft.resetting {
            rows.push(div().p(px(12.)).child(self.reset_confirm(path, name, cx)).into_any_element());
        }
        rows.push(remove.into_any_element());
        group(None, rows, None)
    }

    /// The settings Reset would put back, asking to be confirmed.
    fn reset_confirm(&self, path: &str, name: &str, cx: &mut Context<Self>) -> Div {
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

    /// The project's mark, name, folder and what git says of it, centred over its settings.
    fn repo_hero(&self, name: &str, cx: &mut Context<Self>) -> Div {
        let f = &self.repo_form;
        let (title, copy) = match &f.draft.path {
            Some(p) => match hosts::split(p) {
                (Some(h), path) => (format!("{}:{path}", self.host_name(h)), Some(path.to_string())),
                (None, _) => (tilde(p), Some(p.clone())),
            },
            None => ("No folder chosen".into(), None),
        };
        let copied = f.draft.copied;
        let copy = copy.map(|path| {
            div()
                .id("repo-copy-path")
                .size(px(20.))
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .rounded(px(5.))
                .cursor_pointer()
                .hover(|d| d.bg(FILL_3))
                .child(if copied { icon("check", 12., SUCCESS_TEXT) } else { icon("copy", 12., TEXT_4) })
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.copy_repo_path(path.clone(), cx)))
        });
        let name_field = div()
            .mt(px(10.))
            .w(px(360.))
            .h(px(40.))
            .px(px(10.))
            .flex()
            .items_center()
            .rounded(px(8.))
            .hover(|d| d.bg(FILL_1))
            .child(div().flex_1().child(Input::new(&f.name).appearance(false).p_0().h(px(36.)).line_height(px(30.)).text_size(px(22.)).font_weight(FontWeight::BOLD).text_center()));
        let path = div().flex().items_center().gap(px(4.)).font_family(MONO).text_size(px(12.)).text_color(TEXT_3).child(div().max_w(px(480.)).truncate().child(title)).children(copy);
        let facts = div().pt(px(6.)).flex().flex_wrap().justify_center().gap(px(6.)).children(folder_facts(f.draft.probe.as_ref()).into_iter().map(fact_chip));
        div().pb(px(4.)).flex().flex_col().items_center().gap(px(6.)).child(self.mark_picker(name, cx)).child(name_field).child(path).child(facts)
    }

    pub fn repo_view(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let name = self.repo_form_name(cx);
        let editing = self.repo_form.draft.editing.clone();
        let mut body: Vec<AnyElement> = Vec::new();
        if self.repo_form.draft.source == Source::Host && editing.is_none() {
            body.push(self.host_browser(cx).into_any_element());
        }
        body.push(self.repo_hero(&name, cx).into_any_element());
        let accounts = self.project_account_rows(cx);
        if !accounts.is_empty() {
            body.push(group(Some("Sessions"), accounts, Some("New chats in this project start on these accounts.")).into_any_element());
        }
        let f = &self.repo_form;
        if !f.draft.plain_folder() {
            let scripts = vec![script_row("When one is created", "Runs in the new worktree", &f.setup), script_row("When one is deleted", "Runs before it\u{2019}s removed", &f.teardown)];
            body.push(group(Some("Worktrees"), scripts, Some("Leave empty to skip.")).into_any_element());
        }
        if let Some(p) = &editing {
            body.push(self.project_actions(p, &name, cx).into_any_element());
        }
        let unsaved = self.repo_unsaved(cx);
        let note = match &editing {
            Some(_) if unsaved => div().flex().items_center().gap(px(7.)).child(ui::dot(6., WAITING_DOT)).child("Unsaved changes"),
            None if !name.is_empty() => div().truncate().child(format!("Adds {name} to the project rail")),
            _ => div(),
        };
        let ready = self.repo_ready(cx) && unsaved;
        let label = if editing.is_some() { "Save" } else { "Add project" };
        let cancel = ui::large(ui::button("repo-cancel", Variant::Ghost, None, "Cancel").text_color(TEXT))
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.close_overlay(window, cx)));
        let submit = ui::large(ui::button("repo-submit", Variant::Primary, None, label))
            .when(ready, |d| d.on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.save_repo(window, cx))))
            .when(!ready, |d| d.opacity(0.5).cursor_default());
        let close = ui::icon_button("repo-close", "x").on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.close_overlay(window, cx)));
        let title = if editing.is_some() { "Project settings" } else { "Add project" };
        let header = div().flex().items_center().child(div().flex_1().text_size(px(13.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT_3).child(title)).child(close);
        // The settings scroll between a fixed header and footer when the window is too short for them.
        let max_h = window.viewport_size().height - px(2. * SHEET_TOP);
        let scroll = div().id("repo-scroll").flex_1().min_h_0().overflow_y_scroll().px(px(24.)).flex().flex_col().gap(px(20.)).children(body);
        div().absolute().top(px(SHEET_TOP)).left_0().right_0().flex().justify_center().child(
            ui::pop(div().w(px(600.)).max_h(max_h).pt(px(14.)).pb(px(20.)).flex().flex_col().gap(px(14.)))
                .rounded(px(R_DIALOG))
                .occlude()
                .child(header.px(px(18.)))
                .child(scroll)
                .child(footer(note, cancel, submit).px(px(24.))),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{Fact, Probe, RepoDraft, Tone, folder_facts, project_changes};
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
    fn a_repository_reads_as_its_branch_whether_it_has_changes_and_its_remotes() {
        let labels = |probe: Probe| folder_facts(Some(&probe)).into_iter().map(|f: Fact| (f.label, f.tone)).collect::<Vec<_>>();
        let plain = |l: &str| (l.to_string(), Tone::Plain);
        assert_eq!(labels(Probe::Git { branch: "develop".into(), clean: false, remotes: 1 }), [plain("develop"), ("Uncommitted changes".into(), Tone::Dirty), plain("1 remote")]);
        assert_eq!(labels(Probe::Git { branch: "main".into(), clean: true, remotes: 0 }), [plain("main"), ("Clean".into(), Tone::Good), plain("No remote")]);
        assert_eq!(labels(Probe::NotGit), [plain("Not a git repository")]);
    }

    #[test]
    fn an_edited_project_is_unsaved_only_once_it_differs_from_how_it_opened_and_a_new_one_always_is() {
        let opened = RepoConfig { name: "app".into(), setup: "pnpm install".into(), ..RepoConfig::default() };
        let edited = RepoDraft { saved: Some(opened.clone()), ..local(None) };
        assert!(!edited.unsaved(&opened));
        assert!(edited.unsaved(&RepoConfig { setup: String::new(), ..opened.clone() }));
        assert!(local(None).unsaved(&opened));
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
