use crate::desktop::Desktop;
use crate::desktop::chrome::Overlay;
use crate::modals::form::{footer, home};
use crate::modals::host_finder::Opener;
use crate::modals::new_session::picker::machine_rows;
use crate::settings::ssh_hosts::link_color;
use crate::sidebar::sections::group_mark;
use crate::spaces::header::glyph;
use crate::spaces::{Reach, Space};
use crate::util::{tilde_in, untilde_in};
use hosts::remote;
use std::collections::HashMap;
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

/// Where `typed` points on a machine whose home is `home`: `~` expanded once the home is known, no trailing slash.
fn folder(typed: &str, home: Option<&str>) -> String {
    let typed = typed.trim();
    home.map_or_else(|| typed.to_string(), |home| untilde_in(typed, home)).trim_end_matches('/').to_string()
}

/// The folder a clone starts in: on this Mac the one chosen in Settings or `~/code`; on a host `~/code` under its own home, as the Mac's choice is no path there.
fn default_root(host: Option<&str>, home: &str, mac_root: &str) -> String {
    if host.is_some() { format!("{home}/code") } else { mac_root.to_string() }
}

/// The key the cloned folder is saved under as a project: its path on this Mac, or its host's key for it.
fn clone_key(host: Option<&str>, path: &str) -> String {
    host.map_or_else(|| path.to_string(), |h| hosts::place(h, path))
}

/// The folder `git clone` is given: its path on the machine it runs on, which doesn't know the desktop's host key.
fn git_dest(key: &str) -> &str {
    hosts::split(key).1
}

/// Whether `path` names a folder from a root: `/`, or a Windows drive on a host.
fn is_full_path(path: &str) -> bool {
    path.starts_with('/') || hosts::on_windows(path)
}

fn check(url: &str, dest: &str, host: Option<&str>, projects: &[String]) -> Check {
    if url.trim().is_empty() {
        return Check::Empty;
    }
    match parse_repo(url) {
        None => Check::Invalid,
        Some(_) if !is_full_path(dest) => Check::NoFolder,
        Some(_) if projects.contains(&clone_key(host, dest)) => Check::Taken,
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
    /// The machine the repository is cloned onto.
    machine: Space,
    pub(crate) machine_menu: bool,
    /// Each host's home, as its pocketd said, fetched when the host is first picked.
    homes: HashMap<String, String>,
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
        (Self { url, path, root: None, path_edited: false, group: None, group_menu: false, machine: Space::Local, machine_menu: false, homes: HashMap::new(), busy: false, error: None }, subs)
    }
}

impl Desktop {
    pub(crate) fn clone_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open(Overlay::Clone, window, cx);
    }

    pub(crate) fn reset_clone_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let space = self.space();
        let machine = if self.space_reach(&space) == Reach::Online { space } else { Space::Local };
        let f = &mut self.clone_form;
        (f.root, f.path_edited, f.group, f.group_menu, f.machine, f.machine_menu) = (None, false, None, false, machine, false);
        f.url.update(cx, |s, cx| {
            s.set_value("", window, cx);
            s.focus(window, cx);
        });
        self.fetch_clone_home(window, cx);
        self.clone_url_changed(window, cx);
    }

    /// The home folder of the machine cloned onto, once known.
    fn clone_home(&self) -> Option<String> {
        match &self.clone_form.machine {
            Space::Local => Some(home()),
            Space::Host(id) => self.clone_form.homes.get(id).cloned(),
        }
    }

    /// Asks a host for its home, off the UI thread, once; the folder names itself when it arrives.
    pub(crate) fn fetch_clone_home(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Space::Host(id) = self.clone_form.machine.clone() else { return };
        let Some(client) = remote::connected(&id).filter(|_| !self.clone_form.homes.contains_key(&id)) else { return };
        cx.spawn_in(window, async move |this, cx| {
            let home = cx.background_executor().spawn(async move { client.home() }).await;
            this.update_in(cx, |d, window, cx| {
                if let Ok(home) = home {
                    d.clone_form.homes.insert(id, home);
                    d.clone_url_changed(window, cx);
                }
            })
            .ok();
        })
        .detach();
    }

    /// Names the folder after the repository, in the chosen or clone folder, unless one was typed.
    pub(crate) fn clone_url_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.clone_form.error = None;
        if !self.clone_form.path_edited {
            let home = self.clone_home();
            let host = self.clone_form.machine.host().map(str::to_string);
            let root = self.clone_form.root.clone().or_else(|| home.as_deref().map(|h| default_root(host.as_deref(), h, &self.store.clone_root(h))));
            let shown = |path: &str| tilde_in(path, home.as_deref().unwrap_or_default());
            let dest = root.as_deref().zip(parse_repo(&self.clone_form.url.read(cx).value())).map(|(root, r)| shown(&format!("{root}/{}", r.name))).unwrap_or_default();
            let placeholder = root.map_or_else(String::new, |root| format!("{}/repo", shown(&root)));
            self.clone_form.path.update(cx, |s, cx| {
                s.set_value(dest, window, cx);
                s.set_placeholder(placeholder, window, cx);
            });
        }
        cx.notify();
    }

    /// The repository's name in the typed URL, empty while it names none.
    pub(crate) fn clone_repo_name(&self, cx: &App) -> String {
        parse_repo(&self.clone_form.url.read(cx).value()).map(|r| r.name).unwrap_or_default()
    }

    /// The folder typed, as a path on the machine cloned onto.
    fn clone_dest(&self, cx: &App) -> String {
        folder(&self.clone_form.path.read(cx).value(), self.clone_home().as_deref())
    }

    fn clone_check(&self, cx: &App) -> Check {
        check(&self.clone_form.url.read(cx).value(), &self.clone_dest(cx), self.clone_form.machine.host(), &self.store.projects)
    }

    fn clone_ready(&self, cx: &App) -> bool {
        !self.clone_form.busy && matches!(self.clone_check(cx), Check::Ready(_))
    }

    /// Clones onto `space`, with the folder named afresh for its home.
    fn pick_clone_machine(&mut self, space: Space, window: &mut Window, cx: &mut Context<Self>) {
        let f = &mut self.clone_form;
        (f.machine, f.machine_menu, f.root, f.path_edited) = (space, false, None, false);
        self.fetch_clone_home(window, cx);
        self.clone_url_changed(window, cx);
    }

    /// The folder the host finder picked to clone into, on `host`.
    pub(crate) fn clone_into(&mut self, host: String, dir: String, window: &mut Window, cx: &mut Context<Self>) {
        self.pick_clone_machine(Space::Host(host), window, cx);
        self.clone_form.root = Some(dir);
        self.clone_url_changed(window, cx);
        self.start_clone(window, cx);
    }

    pub(crate) fn start_clone(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.clone_ready(cx) {
            return;
        }
        let host = self.clone_form.machine.host().map(str::to_string);
        let path = self.clone_dest(cx);
        let dest = clone_key(host.as_deref(), &path);
        let url = self.clone_form.url.read(cx).value().trim().to_string();
        let group = self.clone_form.group;
        (self.clone_form.busy, self.clone_form.error) = (true, None);
        let task = cx.background_executor().spawn({
            let dest = dest.clone();
            async move { git::clone(&hosts::runner_for(&dest), &url, git_dest(&dest)) }
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
                        if host.is_some() {
                            if matches!(d.overlay, Some(Overlay::Clone)) {
                                d.close_overlay(window, cx);
                            }
                            d.open_chosen_folder(dest, window, cx);
                        } else {
                            let cfg = d.new_repo_config(&dest);
                            d.add_repo(dest, cfg, window, cx);
                        }
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
                let dest = clone_key(self.clone_form.machine.host(), &self.clone_dest(cx));
                let dest_name = dest.clone();
                let show = ui::link("clone-show", "Show it").on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.close_overlay(window, cx);
                    this.select_project(dest.clone(), cx);
                }));
                line.child(format!("{} is already in your projects.", self.repo_name(&dest_name))).child(show)
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
            .children(picked.and_then(group_mark))
            .child(div().flex_1().text_size(px(13.5)).child(picked.map_or("No group".to_string(), |g| g.name.clone())))
            .child(icon("chevron-down", 12., TEXT_4));
        let button = ui::popover_trigger(button, open, cx.listener(|this, _: &MouseDownEvent, _, cx| {
            this.clone_form.group_menu = true;
            cx.notify();
        }));
        let pick = |id: Option<u32>, lead: AnyElement, label: &str| {
            ui::menu_pick(SharedString::from(format!("clone-group-{id:?}")), lead, label, f.group == id).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                (this.clone_form.group, this.clone_form.group_menu) = (id, false);
                cx.notify();
            }))
        };
        let mut rows = vec![pick(None, div().into_any_element(), "No group")];
        rows.extend(groups.iter().map(|g| pick(Some(g.id), div().children(group_mark(g)).into_any_element(), &g.name)));
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

    /// The machine cloned onto, in the folder's field, and its menu.
    fn clone_machine_chip(&self, cx: &mut Context<Self>) -> Div {
        let f = &self.clone_form;
        let link = f.machine.host().and_then(|id| self.fleet.hosts.links.get(id));
        let open = f.machine_menu;
        let chip = div()
            .id("clone-machine")
            .h(px(24.))
            .px(px(7.))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(6.))
            .rounded(px(6.))
            .text_size(px(12.5))
            .text_color(TEXT_2)
            .when(!f.busy, |d| d.cursor_pointer().hover(|s| s.bg(FILL_3)))
            .when(open, |d| d.bg(FILL_3))
            .child(icon(glyph(&f.machine), 12., TEXT_3))
            .child(self.space_name(&f.machine))
            .when(f.machine.host().is_some(), |d| d.child(ui::dot(6., link_color(link))))
            .child(icon("chevron-down", 11., TEXT_4));
        let chip = if f.busy {
            chip
        } else {
            ui::popover_trigger(chip, open, cx.listener(|this, _: &MouseDownEvent, _, cx| {
                this.clone_form.machine_menu = true;
                cx.notify();
            }))
        };
        let menu = open.then(|| {
            let rows = machine_rows(self, "clone-machine", &f.machine, Self::pick_clone_machine, cx);
            let menu = ui::pop(div().id("clone-machine-menu"))
                .w(px(240.))
                .p(px(6.))
                .flex()
                .flex_col()
                .occlude()
                .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                    this.clone_form.machine_menu = false;
                    cx.notify();
                }))
                .child(ui::menu_label("Clone onto"))
                .children(rows);
            ui::dropdown_right(30., ui::menu_in("clone-machine-menu-in", menu))
        });
        div().relative().child(chip).children(menu)
    }

    pub fn clone_view(&mut self, _: &mut Window, cx: &mut Context<Self>) -> Div {
        let check = self.clone_check(cx);
        let f = &self.clone_form;
        let ready = self.clone_ready(cx);
        let url = ui::field_box()
            .when(check == Check::Invalid, |d| d.shadow(vec![ui::ring(FAILED, 1.)]))
            .child(icon("globe", 13., TEXT_3))
            .child(div().flex_1().font_family(MONO).child(Input::new(&f.url).appearance(false).p_0().text_size(px(13.))));
        let host = f.machine.host().map(str::to_string);
        let choose = ui::button("clone-choose", Variant::Secondary, None, "Choose…").h(px(26.)).on_click(cx.listener(|this, _: &ClickEvent, window, cx| match this.clone_form.machine.host().map(str::to_string) {
            Some(host) => {
                let start = this.clone_form.root.clone();
                this.open_host_finder(host, start, Opener::CloneInto, window, cx);
            }
            None => this.pick_path(true, window, cx, |d, paths, window, cx| {
                let Some(dir) = paths.into_iter().next() else { return };
                (d.clone_form.root, d.clone_form.path_edited) = (Some(dir.to_string_lossy().trim_end_matches('/').to_string()), false);
                d.clone_url_changed(window, cx);
            }),
        }));
        let path = ui::field_box()
            .pr(px(6.))
            .gap(px(8.))
            .child(icon("folder", 13., TEXT_3))
            .child(div().flex_1().font_family(MONO).child(Input::new(&f.path).appearance(false).p_0().text_size(px(13.))))
            .child(self.clone_machine_chip(cx))
            .child(choose);
        let cloning = f.busy.then(|| {
            let name = self.space_name(&f.machine);
            let on = if host.is_some() { format!("Cloning on {name}") } else { "Cloning".to_string() };
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .text_size(px(12.5))
                .text_color(TEXT_2)
                .child(spinner("clone-progress", 13., TEXT_3))
                .child(on)
                .child(div().min_w_0().truncate().font_family(MONO).text_color(TEXT_3).child(f.path.read(cx).value().to_string()))
        });
        let field = |label: &str, body: Div| div().flex().flex_col().gap(px(6.)).child(ui::field_label(label.to_string())).child(body);
        let label = if f.busy { "Cloning…" } else { "Clone" };
        let cancel = ui::large(ui::button("clone-cancel", Variant::Secondary, None, "Cancel")).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.close_overlay(window, cx)));
        let submit = ui::large(ui::button("clone-submit", Variant::Primary, None, label))
            .when(ready, |d| d.on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.start_clone(window, cx))))
            .when(!ready, |d| d.opacity(0.5).cursor_default());
        let close = ui::icon_button("clone-close", "x").on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.close_overlay(window, cx)));
        let mut body = vec![
            field("Repository URL", url).child(self.clone_hint(&check, cx)).into_any_element(),
            field("Clone into", path).children(cloning).into_any_element(),
        ];
        body.extend(self.clone_group_field(cx).map(IntoElement::into_any_element));
        body.push(footer(div(), cancel, submit).into_any_element());
        ui::modal("Clone from URL", 480., 96., close, body)
    }
}

#[cfg(test)]
mod tests {
    use super::{Check, Repo, check, clone_key, default_root, folder, git_dest, parse_repo};

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
        assert_eq!(check("", "/code/app", None, &projects), Check::Empty);
        assert_eq!(check("nope", "/code/app", None, &projects), Check::Invalid);
        assert_eq!(check("https://github.com/org/app", "/code/app", None, &projects), Check::Taken);
        assert_eq!(check("https://github.com/org/app", "/code/app-2", None, &projects), Check::Ready(repo("github.com", "org", "app").unwrap()));
    }

    #[test]
    fn a_clone_needs_a_full_path_and_a_trailing_slash_names_the_same_folder() {
        let projects = vec!["/code/app".to_string()];
        assert_eq!(folder(" /code/app/ ", Some("/Users/me")), "/code/app");
        assert_eq!(check("https://github.com/org/app", &folder("/code/app/", Some("/Users/me")), None, &projects), Check::Taken);
        for dest in ["", "code/app", "host:/code/app"] {
            assert_eq!(check("https://github.com/org/app", dest, None, &projects), Check::NoFolder, "{dest}");
        }
    }

    #[test]
    fn a_clone_onto_a_host_is_saved_under_the_hosts_key_and_taken_only_by_a_project_there() {
        assert_eq!(clone_key(Some("devbox"), "/home/dev/code/app"), "//devbox/home/dev/code/app");
        assert_eq!(clone_key(None, "/Users/me/code/app"), "/Users/me/code/app");
        let projects = vec!["//devbox/home/dev/code/app".to_string(), "/home/dev/code/web".to_string()];
        let url = "https://github.com/org/app";
        assert_eq!(check(url, "/home/dev/code/app", Some("devbox"), &projects), Check::Taken);
        assert_eq!(check(url, "/home/dev/code/app", Some("build"), &projects), Check::Ready(repo("github.com", "org", "app").unwrap()));
        assert_eq!(check("https://github.com/org/web", "/home/dev/code/web", Some("devbox"), &projects), Check::Ready(repo("github.com", "org", "web").unwrap()));
        assert_eq!(check(url, "C:/src/app", Some("win"), &projects), Check::Ready(repo("github.com", "org", "app").unwrap()));
    }

    #[test]
    fn a_host_is_told_to_clone_into_its_own_path_not_the_desktops_key() {
        assert_eq!(git_dest(&clone_key(Some("devbox"), "/home/dev/code/app")), "/home/dev/code/app");
        assert_eq!(git_dest("/Users/me/code/app"), "/Users/me/code/app");
    }

    #[test]
    fn the_default_folder_on_a_host_is_code_under_its_own_home_not_this_macs_choice() {
        assert_eq!(default_root(Some("devbox"), "/home/dev", "/Users/me/work"), "/home/dev/code");
        assert_eq!(default_root(None, "/Users/me", "/Users/me/work"), "/Users/me/work");
        assert_eq!(folder("~/code/app", Some("/home/dev")), "/home/dev/code/app");
    }

    #[test]
    fn a_tilde_path_is_not_a_full_path_while_the_hosts_home_is_unknown() {
        assert_eq!(check("https://github.com/org/app", &folder("~/code/app", None), Some("devbox"), &[]), Check::NoFolder);
    }
}
