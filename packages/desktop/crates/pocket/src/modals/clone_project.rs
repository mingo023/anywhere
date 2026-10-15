use crate::desktop::Desktop;
use crate::desktop::chrome::Overlay;
use crate::modals::form::{footer, home};
use crate::sidebar::sections::color;
use crate::util::{tilde, untilde};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;
use ui::Variant;

/// A repository a clone URL names.
#[derive(Debug, PartialEq)]
struct Repo {
    host: String,
    owner: String,
    name: String,
}

/// What the typed URL and folder allow.
#[derive(Debug, PartialEq)]
enum Check {
    Empty,
    Invalid,
    /// No full path to clone into.
    NoFolder,
    /// The folder is already a project.
    Taken,
    Ready(Repo),
}

/// Reads `https://host/owner/name`, `git@host:owner/name.git` or `ssh://git@host/owner/name`.
fn parse_repo(url: &str) -> Option<Repo> {
    let url = url.trim().trim_end_matches('/');
    if url.contains(char::is_whitespace) {
        return None;
    }
    let rest = ["https://", "http://", "ssh://", "git://"].iter().find_map(|s| url.strip_prefix(s)).unwrap_or(url);
    let rest = rest.split_once('@').map_or(rest, |(_, r)| r);
    let (host, path) = rest.split_once(['/', ':'])?;
    let path = path.strip_suffix(".git").unwrap_or(path);
    let (owner, name) = path.rsplit_once('/')?;
    let named = |s: &str| !s.is_empty() && !s.starts_with('.');
    (host.contains('.') && named(owner) && named(name)).then(|| Repo { host: host.into(), owner: owner.into(), name: name.into() })
}

/// Where `typed` points: home expanded, no trailing slash.
fn folder(typed: &str) -> String {
    untilde(typed.trim()).trim_end_matches('/').to_string()
}

fn check(url: &str, dest: &str, projects: &[String]) -> Check {
    if url.trim().is_empty() {
        return Check::Empty;
    }
    match parse_repo(url) {
        None => Check::Invalid,
        Some(_) if !dest.starts_with('/') => Check::NoFolder,
        Some(_) if projects.iter().any(|p| p == dest) => Check::Taken,
        Some(r) => Check::Ready(r),
    }
}

pub struct CloneForm {
    pub(crate) url: Entity<InputState>,
    path: Entity<InputState>,
    /// The folder Choose… picked to clone into, over the clone folder.
    root: Option<String>,
    /// The folder was typed, so the URL no longer names it.
    path_edited: bool,
    group: Option<u32>,
    pub(crate) group_menu: bool,
    busy: bool,
    error: Option<String>,
}

impl CloneForm {
    pub fn new(window: &mut Window, cx: &mut Context<Desktop>) -> (Self, Vec<Subscription>) {
        let url = cx.new(|cx| InputState::new(window, cx).placeholder("https://github.com/owner/repo"));
        let path = cx.new(|cx| InputState::new(window, cx));
        let subs = vec![
            cx.subscribe_in(&url, window, |this, _, ev: &InputEvent, window, cx| match ev {
                InputEvent::Change => this.clone_url_changed(window, cx),
                InputEvent::PressEnter { .. } => this.start_clone(window, cx),
                _ => {}
            }),
            cx.subscribe_in(&path, window, |this, path, ev: &InputEvent, window, cx| match ev {
                InputEvent::Change => {
                    this.clone_form.error = None;
                    this.clone_form.path_edited = !path.read(cx).value().trim().is_empty();
                    cx.notify();
                }
                InputEvent::PressEnter { .. } => this.start_clone(window, cx),
                _ => {}
            }),
        ];
        (Self { url, path, root: None, path_edited: false, group: None, group_menu: false, busy: false, error: None }, subs)
    }
}

impl Desktop {
    pub(crate) fn clone_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open(Overlay::Clone, window, cx);
    }

    pub(crate) fn reset_clone_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let f = &mut self.clone_form;
        (f.root, f.path_edited, f.group, f.group_menu) = (None, false, None, false);
        f.url.update(cx, |s, cx| {
            s.set_value("", window, cx);
            s.focus(window, cx);
        });
        self.clone_url_changed(window, cx);
    }

    /// Names the folder after the repository, in the chosen or clone folder, unless one was typed.
    pub(crate) fn clone_url_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.clone_form.error = None;
        if !self.clone_form.path_edited {
            let root = self.clone_form.root.clone().unwrap_or_else(|| self.store.clone_root(&home()));
            let dest = parse_repo(&self.clone_form.url.read(cx).value()).map(|r| tilde(&format!("{root}/{}", r.name))).unwrap_or_default();
            self.clone_form.path.update(cx, |s, cx| {
                s.set_value(dest, window, cx);
                s.set_placeholder(format!("{}/repo", tilde(&root)), window, cx);
            });
        }
        cx.notify();
    }

    fn clone_dest(&self, cx: &App) -> String {
        folder(&self.clone_form.path.read(cx).value())
    }

    fn clone_check(&self, cx: &App) -> Check {
        check(&self.clone_form.url.read(cx).value(), &self.clone_dest(cx), &self.store.projects)
    }

    fn clone_ready(&self, cx: &App) -> bool {
        !self.clone_form.busy && matches!(self.clone_check(cx), Check::Ready(_))
    }

    fn start_clone(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.clone_ready(cx) {
            return;
        }
        let dest = self.clone_dest(cx);
        let url = self.clone_form.url.read(cx).value().trim().to_string();
        let group = self.clone_form.group;
        (self.clone_form.busy, self.clone_form.error) = (true, None);
        let task = cx.background_executor().spawn({
            let dest = dest.clone();
            async move { git::clone(&hosts::runner_for(&dest), &url, &dest) }
        });
        cx.spawn_in(window, async move |this, cx| {
            let res = task.await;
            this.update_in(cx, |d, window, cx| {
                d.clone_form.busy = false;
                match res {
                    Ok(()) => {
                        if let Some(g) = group {
                            d.store.sections.set_group(&dest, Some(g));
                            d.store.sections.reveal(&dest, None);
                        }
                        let cfg = d.new_repo_config(&dest);
                        d.add_repo(dest, cfg, window, cx);
                    }
                    Err(e) => d.clone_form.error = Some(e),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn clone_hint(&self, check: &Check, cx: &mut Context<Self>) -> Div {
        let line = div().h(px(18.)).flex().items_center().gap(px(6.)).text_size(px(12.5)).text_color(TEXT_4);
        if let Some(e) = &self.clone_form.error {
            return line.text_color(FAILED).child(div().truncate().child(e.clone()));
        }
        match check {
            Check::Empty => line.child("HTTPS or SSH, from GitHub, GitLab or any git host."),
            Check::Invalid => line.text_color(FAILED).child("Paste an HTTPS or SSH URL, like https://github.com/owner/repo"),
            Check::NoFolder => line.text_color(FAILED).child("Clone into a full path, like ~/code/repo"),
            Check::Taken => {
                let dest = self.clone_dest(cx);
                let show = ui::link("clone-show", "Show it").on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.close_overlay(window, cx);
                    this.select_project(dest.clone(), cx);
                }));
                line.child(format!("{} is already in your projects.", self.repo_name(&self.clone_dest(cx)))).child(show)
            }
            Check::Ready(r) => line
                .text_color(TEXT_2)
                .child(icon("branch", 12., TEXT_3))
                .child(div().font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child(r.name.clone()))
                .child(format!("from {}/{}", r.host, r.owner)),
        }
    }

    fn clone_group_field(&self, cx: &mut Context<Self>) -> Option<Div> {
        let groups = &self.store.sections.groups;
        if groups.is_empty() {
            return None;
        }
        let f = &self.clone_form;
        let picked = f.group.and_then(|id| self.store.sections.group(id));
        let open = f.group_menu;
        let button = ui::field_box()
            .id("clone-group")
            .cursor_pointer()
            .gap(px(8.))
            .children(picked.map(|g| ui::dot(8., color(g.color))))
            .child(div().flex_1().text_size(px(13.5)).child(picked.map_or("No group".to_string(), |g| g.name.clone())))
            .child(icon("chevron-down", 12., TEXT_4))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.clone_form.group_menu = !open;
                cx.notify();
            }));
        let pick = |id: Option<u32>, lead: AnyElement, label: &str| {
            ui::menu_pick(SharedString::from(format!("clone-group-{id:?}")), lead, label, f.group == id).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                (this.clone_form.group, this.clone_form.group_menu) = (id, false);
                cx.notify();
            }))
        };
        let mut rows = vec![pick(None, div().into_any_element(), "No group")];
        rows.extend(groups.iter().map(|g| pick(Some(g.id), ui::dot(8., color(g.color)).into_any_element(), &g.name)));
        let menu = ui::pop(div().id("clone-group-menu"))
            .w(px(240.))
            .p(px(6.))
            .flex()
            .flex_col()
            .occlude()
            .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                this.clone_form.group_menu = false;
                cx.notify();
            }))
            .children(rows);
        let field = div().relative().child(button).when(open, |d| d.child(ui::dropdown(42., ui::menu_in("clone-group-menu-in", menu))));
        Some(div().flex().flex_col().gap(px(6.)).child(ui::field_label("Group")).child(field))
    }

    pub fn clone_view(&mut self, _: &mut Window, cx: &mut Context<Self>) -> Div {
        let check = self.clone_check(cx);
        let f = &self.clone_form;
        let ready = self.clone_ready(cx);
        let url = ui::field_box()
            .when(check == Check::Invalid, |d| d.shadow(vec![ui::ring(FAILED, 1.)]))
            .child(icon("globe", 13., TEXT_3))
            .child(div().flex_1().font_family(MONO).child(Input::new(&f.url).appearance(false).p_0().text_size(px(13.))));
        let choose = ui::button("clone-choose", Variant::Secondary, None, "Choose…").h(px(26.)).on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
            this.pick_path(true, window, cx, |d, paths, window, cx| {
                let Some(dir) = paths.into_iter().next() else { return };
                (d.clone_form.root, d.clone_form.path_edited) = (Some(dir.to_string_lossy().trim_end_matches('/').to_string()), false);
                d.clone_url_changed(window, cx);
            })
        }));
        let path = ui::field_box()
            .pr(px(6.))
            .child(icon("folder", 13., TEXT_3))
            .child(div().flex_1().font_family(MONO).child(Input::new(&f.path).appearance(false).p_0().text_size(px(13.))))
            .child(choose);
        let field = |label: &str, body: Div| div().flex().flex_col().gap(px(6.)).child(ui::field_label(label.to_string())).child(body);
        let label = if f.busy { "Cloning…" } else { "Clone" };
        let cancel = ui::large(ui::button("clone-cancel", Variant::Ghost, None, "Cancel").text_color(TEXT)).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.close_overlay(window, cx)));
        let submit = ui::large(ui::button("clone-submit", Variant::Primary, None, label))
            .when(ready, |d| d.on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.start_clone(window, cx))))
            .when(!ready, |d| d.opacity(0.5).cursor_default());
        let close = ui::icon_button("clone-close", "x").on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.close_overlay(window, cx)));
        let mut body = vec![
            field("Repository URL", url).child(self.clone_hint(&check, cx)).into_any_element(),
            field("Clone into", path).into_any_element(),
        ];
        body.extend(self.clone_group_field(cx).map(IntoElement::into_any_element));
        body.push(footer(div(), cancel, submit).into_any_element());
        ui::modal("Clone from URL", 480., 96., close, body)
    }
}

#[cfg(test)]
mod tests {
    use super::{Check, Repo, check, folder, parse_repo};

    fn repo(host: &str, owner: &str, name: &str) -> Option<Repo> {
        Some(Repo { host: host.into(), owner: owner.into(), name: name.into() })
    }

    #[test]
    fn https_ssh_and_scp_urls_name_their_host_owner_and_repository() {
        assert_eq!(parse_repo("https://github.com/org/app-ios.git"), repo("github.com", "org", "app-ios"));
        assert_eq!(parse_repo("git@github.com:org/app-ios.git"), repo("github.com", "org", "app-ios"));
        assert_eq!(parse_repo(" https://github.com/org/web/ "), repo("github.com", "org", "web"));
        assert_eq!(parse_repo("ssh://git@gitlab.com/group/sub/api"), repo("gitlab.com", "group/sub", "api"));
    }

    #[test]
    fn text_that_names_no_repository_is_not_a_url() {
        for url in ["app-ios", "https://github.com/org", "/Users/me/code/app", "https://github.com/org/my app"] {
            assert_eq!(parse_repo(url), None, "{url}");
        }
    }

    #[test]
    fn a_clone_into_a_folder_that_is_already_a_project_is_refused() {
        let projects = vec!["/code/app".to_string()];
        assert_eq!(check("", "/code/app", &projects), Check::Empty);
        assert_eq!(check("nope", "/code/app", &projects), Check::Invalid);
        assert_eq!(check("https://github.com/org/app", "/code/app", &projects), Check::Taken);
        assert_eq!(check("https://github.com/org/app", "/code/app-2", &projects), Check::Ready(repo("github.com", "org", "app").unwrap()));
    }

    #[test]
    fn a_clone_needs_a_full_path_and_a_trailing_slash_names_the_same_folder() {
        let projects = vec!["/code/app".to_string()];
        assert_eq!(folder(" /code/app/ "), "/code/app");
        assert_eq!(check("https://github.com/org/app", &folder("/code/app/"), &projects), Check::Taken);
        for dest in ["", "code/app", "host:/code/app"] {
            assert_eq!(check("https://github.com/org/app", dest, &projects), Check::NoFolder, "{dest}");
        }
    }
}
