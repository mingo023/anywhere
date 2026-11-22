use crate::desktop::Desktop;
use crate::desktop::chrome::Overlay;
use crate::modals::form::{footer, home};
use crate::modals::host_finder::Opener;
use crate::modals::new_session::picker::machine_row;
use crate::settings::ssh_hosts::link_color;
use crate::spaces::{Reach, Space};
use crate::util::{tilde_in, untilde_in};
use agents::hosts::Link;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use hosts::remote;
use std::collections::HashMap;
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

/// A machine as the menu and chip draw it; `Desktop` names each again every time it draws the form.
#[derive(Clone)]
pub(crate) struct Machine {
    pub space: Space,
    pub glyph: &'static str,
    pub name: String,
    pub link: Option<Link>,
    pub reach: Reach,
}

/// What `Desktop` hands the form, as it stands when the form draws or is asked to act.
pub(crate) struct CloneInputs {
    pub projects: Vec<String>,
    /// Each project's name, by its key.
    pub names: HashMap<String, String>,
    /// Where a clone on this Mac starts, from Settings.
    pub mac_root: String,
    /// Every machine, in the switcher's order.
    pub machines: Vec<Machine>,
    /// The machine cloned onto.
    pub chosen: Machine,
}

/// What the form asks of `Desktop`, which owns the overlay, the projects and the host finder.
pub(crate) enum CloneEvent {
    Close,
    /// Enter, or Clone: `Desktop` hands the form its inputs again, then it starts.
    Submit,
    ShowProject(String),
    /// Choose… on a host: its folders in the host finder.
    ChooseOnHost {
        host: String,
        start: Option<String>,
    },
    /// Choose… on this Mac: Finder's folder picker.
    ChooseOnMac,
    Cloned {
        dest: String,
        host: Option<String>,
    },
}

/// Clone from URL as its own `Entity`: the URL and folder, the machine cloned onto and the clone's run.
pub struct CloneForm {
    pub(crate) url: Entity<InputState>,
    path: Entity<InputState>,
    /// The folder Choose… picked to clone into, over the clone folder.
    root: Option<String>,
    /// The folder was typed, so the URL no longer names it.
    path_edited: bool,
    /// The machine the repository is cloned onto.
    pub(crate) machine: Space,
    pub(crate) machine_menu: bool,
    /// Each host's home, as its pocketd said, fetched when the host is first picked.
    homes: HashMap<String, String>,
    busy: bool,
    error: Option<String>,
    inputs: Option<CloneInputs>,
    _subs: Vec<Subscription>,
}

impl EventEmitter<CloneEvent> for CloneForm {}

impl CloneForm {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let url = cx.new(|cx| InputState::new(window, cx).placeholder("https://github.com/owner/repo"));
        let path = cx.new(|cx| InputState::new(window, cx));
        let subs = vec![
            cx.subscribe_in(&url, window, |this, _, ev: &InputEvent, window, cx| match ev {
                InputEvent::Change => this.url_changed(window, cx),
                InputEvent::PressEnter { .. } => cx.emit(CloneEvent::Submit),
                _ => {}
            }),
            cx.subscribe_in(&path, window, |this, path, ev: &InputEvent, _, cx| match ev {
                InputEvent::Change => {
                    this.error = None;
                    this.path_edited = !path.read(cx).value().trim().is_empty();
                    cx.notify();
                }
                InputEvent::PressEnter { .. } => cx.emit(CloneEvent::Submit),
                _ => {}
            }),
        ];
        Self {
            url,
            path,
            root: None,
            path_edited: false,
            machine: Space::Local,
            machine_menu: false,
            homes: HashMap::new(),
            busy: false,
            error: None,
            inputs: None,
            _subs: subs,
        }
    }

    pub fn show(&mut self, inputs: CloneInputs) {
        self.inputs = Some(inputs);
    }

    pub(crate) fn close_menus(&mut self) {
        self.machine_menu = false;
    }

    pub(crate) fn reset(&mut self, machine: Space, window: &mut Window, cx: &mut Context<Self>) {
        (self.root, self.path_edited, self.machine, self.machine_menu) = (None, false, machine, false);
        self.url.update(cx, |s, cx| {
            s.set_value("", window, cx);
            s.focus(window, cx);
        });
        self.fetch_home(window, cx);
        self.url_changed(window, cx);
    }

    /// The home folder of the machine cloned onto, once known.
    fn home(&self) -> Option<String> {
        match &self.machine {
            Space::Local => Some(home()),
            Space::Host(id) => self.homes.get(id).cloned(),
        }
    }

    /// Asks a host for its home, off the UI thread, once; the folder names itself when it arrives.
    pub(crate) fn fetch_home(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Space::Host(id) = self.machine.clone() else { return };
        let Some(client) = remote::connected(&id).filter(|_| !self.homes.contains_key(&id)) else { return };
        cx.spawn_in(window, async move |this, cx| {
            let home = cx.background_executor().spawn(async move { client.home() }).await;
            this.update_in(cx, |f, window, cx| {
                if let Ok(home) = home {
                    f.homes.insert(id, home);
                    f.url_changed(window, cx);
                }
            })
            .ok();
        })
        .detach();
    }

    /// Names the folder after the repository, in the chosen or clone folder, unless one was typed.
    pub(crate) fn url_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.error = None;
        if !self.path_edited {
            let home = self.home();
            let host = self.machine.host().map(str::to_string);
            let mac_root = self.inputs.as_ref().map(|i| i.mac_root.as_str()).unwrap_or_default();
            let root = self.root.clone().or_else(|| home.as_deref().map(|h| default_root(host.as_deref(), h, mac_root)));
            let shown = |path: &str| tilde_in(path, home.as_deref().unwrap_or_default());
            let dest = root.as_deref().zip(parse_repo(&self.url.read(cx).value())).map(|(root, r)| shown(&format!("{root}/{}", r.name))).unwrap_or_default();
            let placeholder = root.map_or_else(String::new, |root| format!("{}/repo", shown(&root)));
            self.path.update(cx, |s, cx| {
                s.set_value(dest, window, cx);
                s.set_placeholder(placeholder, window, cx);
            });
        }
        cx.notify();
    }

    /// The repository's name in the typed URL, empty while it names none.
    pub(crate) fn repo_name(&self, cx: &App) -> String {
        parse_repo(&self.url.read(cx).value()).map(|r| r.name).unwrap_or_default()
    }

    /// The folder typed, as a path on the machine cloned onto.
    fn dest(&self, cx: &App) -> String {
        folder(&self.path.read(cx).value(), self.home().as_deref())
    }

    fn check(&self, cx: &App) -> Check {
        let projects = self.inputs.as_ref().map_or(&[][..], |i| i.projects.as_slice());
        check(&self.url.read(cx).value(), &self.dest(cx), self.machine.host(), projects)
    }

    fn ready(&self, cx: &App) -> bool {
        !self.busy && matches!(self.check(cx), Check::Ready(_))
    }

    /// Clones onto `space`, with the folder named afresh for its home.
    fn pick_machine(&mut self, space: Space, window: &mut Window, cx: &mut Context<Self>) {
        (self.machine, self.machine_menu, self.root, self.path_edited) = (space, false, None, false);
        self.fetch_home(window, cx);
        self.url_changed(window, cx);
    }

    /// The folder Choose… picked on this Mac.
    pub(crate) fn root_chosen(&mut self, dir: String, window: &mut Window, cx: &mut Context<Self>) {
        (self.root, self.path_edited) = (Some(dir), false);
        self.url_changed(window, cx);
    }

    /// The folder the host finder picked to clone into, on `host`.
    pub(crate) fn clone_into(&mut self, host: String, dir: String, window: &mut Window, cx: &mut Context<Self>) {
        self.pick_machine(Space::Host(host), window, cx);
        self.root = Some(dir);
        self.url_changed(window, cx);
        self.start(window, cx);
    }

    pub(crate) fn start(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.ready(cx) {
            return;
        }
        let host = self.machine.host().map(str::to_string);
        let path = self.dest(cx);
        let dest = clone_key(host.as_deref(), &path);
        let url = self.url.read(cx).value().trim().to_string();
        (self.busy, self.error) = (true, None);
        let task = cx.background_executor().spawn({
            let dest = dest.clone();
            async move { git::clone(&hosts::runner_for(&dest), &url, git_dest(&dest)) }
        });
        cx.spawn_in(window, async move |this, cx| {
            let res = task.await;
            this.update_in(cx, |f, _, cx| {
                f.busy = false;
                match res {
                    Ok(()) => cx.emit(CloneEvent::Cloned { dest, host }),
                    Err(e) => f.error = Some(e),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn hint(&self, check: &Check, cx: &mut Context<Self>) -> Div {
        let line = div().h(px(18.)).flex().items_center().gap(px(6.)).text_size(px(12.5)).text_color(TEXT_4);
        if let Some(e) = &self.error {
            return line.text_color(FAILED).child(div().truncate().child(e.clone()));
        }
        match check {
            Check::Empty => line.child("HTTPS or SSH, from GitHub, GitLab or any git host."),
            Check::Invalid => line.text_color(FAILED).child("Paste an HTTPS or SSH URL, like https://github.com/owner/repo"),
            Check::NoFolder => line.text_color(FAILED).child("Clone into a full path, like ~/code/repo"),
            Check::Taken => {
                let dest = clone_key(self.machine.host(), &self.dest(cx));
                let name = self.inputs.as_ref().and_then(|i| i.names.get(&dest).cloned()).unwrap_or_default();
                let target = dest.clone();
                let show =
                    ui::link("clone-show", "Show it").on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.emit(CloneEvent::ShowProject(target.clone()))));
                line.child(format!("{name} is already in your projects.")).child(show)
            }
            Check::Ready(r) => line
                .text_color(TEXT_2)
                .child(icon("branch", 12., TEXT_3))
                .child(div().font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child(r.name.clone()))
                .child(format!("from {}/{}", r.host, r.owner)),
        }
    }

    /// The machine cloned onto, in the folder's field, and its menu.
    fn machine_chip(&self, inputs: &CloneInputs, cx: &mut Context<Self>) -> Div {
        let chosen = &inputs.chosen;
        let open = self.machine_menu;
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
            .when(!self.busy, |d| d.cursor_pointer().hover(|s| s.bg(FILL_3)))
            .when(open, |d| d.bg(FILL_3))
            .child(icon(chosen.glyph, 12., TEXT_3))
            .child(chosen.name.clone())
            .when(self.machine.host().is_some(), |d| d.child(ui::dot(6., link_color(chosen.link.as_ref()))))
            .child(icon("chevron-down", 11., TEXT_4));
        let chip = if self.busy {
            chip
        } else {
            ui::popover_trigger(
                chip,
                open,
                cx.listener(|this, _: &MouseDownEvent, _, cx| {
                    this.machine_menu = true;
                    cx.notify();
                }),
            )
        };
        let menu = open.then(|| {
            let rows = inputs.machines.iter().enumerate().map(|(i, m)| {
                let row = machine_row(("clone-machine", i), &m.space, m.glyph, &m.name, m.link.as_ref(), m.reach, m.space == self.machine)
                    .when(m.reach != Reach::Online, |d| d.opacity(0.5));
                let space = m.space.clone();
                row.when(m.reach == Reach::Online, |d| {
                    d.on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.pick_machine(space.clone(), window, cx)))
                })
                .into_any_element()
            });
            let menu = ui::pop(div().id("clone-machine-menu"))
                .w(px(240.))
                .p(px(6.))
                .flex()
                .flex_col()
                .occlude()
                .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                    this.machine_menu = false;
                    cx.notify();
                }))
                .child(ui::menu_label("Clone onto"))
                .children(rows);
            ui::dropdown_right(30., ui::menu_in("clone-machine-menu-in", menu))
        });
        div().relative().child(chip).children(menu)
    }
}

impl Render for CloneForm {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(inputs) = self.inputs.as_ref() else { return div() };
        let check = self.check(cx);
        let ready = self.ready(cx);
        let url = ui::field_box()
            .when(check == Check::Invalid, |d| d.shadow(vec![ui::ring(FAILED, 1.)]))
            .child(icon("globe", 13., TEXT_3))
            .child(div().flex_1().font_family(MONO).child(Input::new(&self.url).appearance(false).p_0().text_size(px(13.))));
        let host = self.machine.host().map(str::to_string);
        let choose = ui::button("clone-choose", Variant::Secondary, None, "Choose…").h(px(26.)).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
            match this.machine.host().map(str::to_string) {
                Some(host) => cx.emit(CloneEvent::ChooseOnHost { host, start: this.root.clone() }),
                None => cx.emit(CloneEvent::ChooseOnMac),
            }
        }));
        let path = ui::field_box()
            .pr(px(6.))
            .gap(px(8.))
            .child(icon("folder", 13., TEXT_3))
            .child(div().flex_1().font_family(MONO).child(Input::new(&self.path).appearance(false).p_0().text_size(px(13.))))
            .child(self.machine_chip(inputs, cx))
            .child(choose);
        let cloning = self.busy.then(|| {
            let on = if host.is_some() { format!("Cloning on {}", inputs.chosen.name) } else { "Cloning".to_string() };
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .text_size(px(12.5))
                .text_color(TEXT_2)
                .child(spinner("clone-progress", 13., TEXT_3))
                .child(on)
                .child(div().min_w_0().truncate().font_family(MONO).text_color(TEXT_3).child(self.path.read(cx).value().to_string()))
        });
        let field = |label: &str, body: Div| div().flex().flex_col().gap(px(6.)).child(ui::field_label(label.to_string())).child(body);
        let label = if self.busy { "Cloning…" } else { "Clone" };
        let cancel = ui::large(ui::button("clone-cancel", Variant::Secondary, None, "Cancel"))
            .on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(CloneEvent::Close)));
        let submit = ui::large(ui::button("clone-submit", Variant::Primary, None, label))
            .when(ready, |d| d.on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(CloneEvent::Submit))))
            .when(!ready, |d| d.opacity(0.5).cursor_default());
        let close = ui::icon_button("clone-close", "x").on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(CloneEvent::Close)));
        let body = vec![field("Repository URL", url).child(self.hint(&check, cx)).into_any_element(), field("Clone into", path).children(cloning).into_any_element(), footer(div(), cancel, submit).into_any_element()];
        ui::modal("Clone from URL", 480., 96., close, body)
    }
}

impl Desktop {
    pub(crate) fn clone_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open(Overlay::Clone, window, cx);
    }

    fn machine_of(&self, space: &Space) -> Machine {
        Machine {
            glyph: self.space_glyph(space),
            name: self.space_name(space),
            link: space.host().and_then(|id| self.fleet.hosts.links.get(id)).cloned(),
            reach: self.space_reach(space),
            space: space.clone(),
        }
    }

    fn clone_inputs(&self, chosen: &Space) -> CloneInputs {
        CloneInputs {
            projects: self.store.projects.clone(),
            names: self.store.projects.iter().map(|p| (p.clone(), self.repo_name(p))).collect(),
            mac_root: self.store.clone_root(&home()),
            machines: self.space_order().iter().map(|s| self.machine_of(s)).collect(),
            chosen: self.machine_of(chosen),
        }
    }

    /// Hands the form what it reads of `Desktop`, for the machine it clones onto.
    fn show_clone(&mut self, cx: &mut Context<Self>) {
        let machine = self.clone_form.read(cx).machine.clone();
        let inputs = self.clone_inputs(&machine);
        self.clone_form.update(cx, |f, _| f.show(inputs));
    }

    pub(crate) fn reset_clone_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let space = self.space();
        let machine = if self.space_reach(&space) == Reach::Online { space } else { Space::Local };
        let inputs = self.clone_inputs(&machine);
        self.clone_form.update(cx, |f, cx| {
            f.show(inputs);
            f.reset(machine, window, cx);
        });
    }

    pub(crate) fn clone_view(&mut self, cx: &mut Context<Self>) -> Div {
        self.show_clone(cx);
        div().absolute().inset_0().child(self.clone_form.clone())
    }

    pub(crate) fn start_clone(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.show_clone(cx);
        self.clone_form.update(cx, |f, cx| f.start(window, cx));
    }

    /// Clones into the folder the host finder picked, on `host`.
    pub(crate) fn clone_into(&mut self, host: String, dir: String, window: &mut Window, cx: &mut Context<Self>) {
        self.show_clone(cx);
        self.clone_form.update(cx, |f, cx| f.clone_into(host, dir, window, cx));
    }

    pub(crate) fn on_clone_event(&mut self, _: &Entity<CloneForm>, ev: &CloneEvent, window: &mut Window, cx: &mut Context<Self>) {
        match ev {
            CloneEvent::Close => self.close_overlay(window, cx),
            CloneEvent::Submit => self.start_clone(window, cx),
            CloneEvent::ShowProject(dest) => {
                self.close_overlay(window, cx);
                self.select_project(dest.clone(), cx);
            }
            CloneEvent::ChooseOnHost { host, start } => self.open_host_finder(host.clone(), start.clone(), Opener::CloneInto, window, cx),
            CloneEvent::ChooseOnMac => self.pick_path(true, window, cx, |d, paths, window, cx| {
                let Some(dir) = paths.into_iter().next() else { return };
                let dir = dir.to_string_lossy().trim_end_matches('/').to_string();
                d.clone_form.update(cx, |f, cx| f.root_chosen(dir, window, cx));
            }),
            CloneEvent::Cloned { dest, host } => {
                let dest = dest.clone();
                if host.is_some() {
                    if matches!(self.overlay, Some(Overlay::Clone)) {
                        self.close_overlay(window, cx);
                    }
                    self.open_chosen_folder(dest, window, cx);
                } else {
                    let cfg = self.new_repo_config(&dest);
                    self.add_repo(dest, cfg, window, cx);
                }
                cx.notify();
            }
        }
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
        assert_eq!(
            check("https://github.com/org/web", "/home/dev/code/web", Some("devbox"), &projects),
            Check::Ready(repo("github.com", "org", "web").unwrap())
        );
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
