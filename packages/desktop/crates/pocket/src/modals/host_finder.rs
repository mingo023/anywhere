//! A Finder-like browser of a host's folders, opened over Add project or from the new-session page to pick a project's folder there.

pub(crate) mod nav;

mod columns;
mod detail;
mod footer;
mod header;
mod host_switch;
mod note;
mod places;

use crate::desktop::Desktop;
use crate::modals::add_project::host_folder::child;
use crate::spaces::Space;
use git::Exec as _;
use gpui_kit::*;
use hosts::remote;
use nav::{Ask, Finder, PREVIEW_MAX, Repo, ask, branch_of, remote_of};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;
use theme::*;

/// Who opened the finder, and so where the folder chosen goes.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Opener {
    AddProject,
    NewSession,
    Sidebar,
    /// Clone from URL picking the folder to clone into.
    CloneInto,
}

/// A host the finder's switch lists.
pub(crate) struct HostRow {
    pub id: String,
    pub name: String,
    pub glyph: &'static str,
}

/// What `Desktop` hands the finder each frame.
#[derive(Default)]
pub(crate) struct Side {
    /// The open finder's host name and glyph.
    pub name: String,
    pub glyph: &'static str,
    pub hosts: Vec<HostRow>,
    /// Every project's key.
    pub projects: Vec<String>,
    /// The repository name Clone from URL has typed, while it is the one asking.
    pub clone: Option<String>,
}

pub(crate) enum FinderEvent {
    /// Closed without a folder.
    Closed(Opener),
    Chosen {
        opener: Opener,
        host: String,
        dir: String,
    },
}

/// The column finder over a host's folders, as its own `Entity`.
pub(crate) struct HostFinder {
    finder: Option<Finder>,
    opener: Opener,
    focus: FocusHandle,
    scrolls: HashMap<String, UniformListScrollHandle>,
    preview_scroll: UniformListScrollHandle,
    host_menu: bool,
    opened: usize,
    side: Side,
}

impl EventEmitter<FinderEvent> for HostFinder {}

impl HostFinder {
    pub fn new(cx: &mut App) -> Self {
        Self {
            finder: None,
            opener: Opener::AddProject,
            focus: cx.focus_handle(),
            scrolls: HashMap::new(),
            preview_scroll: UniformListScrollHandle::new(),
            host_menu: false,
            opened: 0,
            side: Side::default(),
        }
    }

    pub(crate) fn host(&self) -> Option<&str> {
        self.finder.as_ref().map(|f| f.host.as_str())
    }

    pub(crate) fn opener(&self) -> Opener {
        self.opener
    }

    pub(crate) fn is_open(&self) -> bool {
        self.finder.is_some()
    }

    pub(crate) fn hand(&mut self, side: Side) {
        self.side = side;
    }

    /// Drops the finder, leaving focus where it is.
    pub(crate) fn drop_finder(&mut self) {
        self.finder = None;
    }

    fn scroll(&mut self, dir: &str) -> UniformListScrollHandle {
        self.scrolls.entry(dir.to_string()).or_default().clone()
    }

    /// The finder, if it's still the one opened `opened`th: one closed and opened again on the same host takes none of the first one's replies.
    fn opened_as(&mut self, opened: usize) -> Option<&mut Finder> {
        let same = self.opened == opened;
        self.finder.as_mut().filter(|_| same)
    }

    /// Opens the finder on `host`, at `start` or the host's home.
    pub(crate) fn open(&mut self, host: String, start: Option<String>, opener: Opener, window: &mut Window, cx: &mut Context<Self>) {
        self.finder = Some(Finder::new(&host));
        self.opener = opener;
        self.host_menu = false;
        self.scrolls.clear();
        self.opened += 1;
        let opened = self.opened;
        window.focus(&self.focus, cx);
        cx.notify();
        let Some(client) = remote::connected(&host) else {
            if let Some(f) = self.finder.as_mut() {
                f.error = Some("Not connected to this host. Reconnect it, then open the folder again.".into());
            }
            return;
        };
        cx.spawn(async move |this, cx| {
            let home = cx.background_executor().spawn(async move { client.home() }).await;
            this.update(cx, |d, cx| {
                let Some(f) = d.opened_as(opened) else { return };
                match home {
                    Ok(home) => f.homed(home, start),
                    Err(e) => f.error = Some(e.to_string()),
                }
                d.fetch(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Opens again a finder that was left without `host`'s home, now that the host answers.
    pub(crate) fn host_back(&mut self, host: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.finder.as_ref().is_some_and(|f| f.host == host && f.home.is_none()) {
            self.open(host.into(), None, self.opener, window, cx);
        }
    }

    /// Closes the finder, leaving what opened it as it was. False when it wasn't open.
    pub(crate) fn close(&mut self, cx: &mut Context<Self>) -> bool {
        if self.finder.take().is_none() {
            return false;
        }
        cx.emit(FinderEvent::Closed(self.opener));
        true
    }

    /// Asks the host, off the UI thread, for whatever the finder now shows but hasn't got.
    fn fetch(&mut self, cx: &mut Context<Self>) {
        let Some(f) = self.finder.as_mut() else { return };
        let Some(client) = remote::connected(&f.host) else { return };
        let (host, opened) = (f.host.clone(), self.opened);
        for dir in f.wanted() {
            let (client, host) = (client.clone(), host.clone());
            cx.spawn(async move |this, cx| {
                let asked = dir.clone();
                let listed = cx.background_executor().spawn(async move { client.list(&asked).map_err(|e| e.to_string()) }).await;
                this.update(cx, |d, cx| {
                    if d.opened_as(opened).is_some_and(|f| f.listed(&host, &dir, listed)) {
                        d.fetch(cx);
                        cx.notify();
                    }
                })
                .ok();
            })
            .detach();
        }
        if let Some(path) = f.preview_wanted() {
            let (client, host) = (client.clone(), host.clone());
            cx.spawn(async move |this, cx| {
                let asked = path.clone();
                let read = cx.background_executor().spawn(async move { client.read(&asked, PREVIEW_MAX).map_err(|e| e.to_string()) }).await;
                this.update(cx, |d, cx| {
                    if let Some(f) = d.opened_as(opened) {
                        f.previewed(&host, &path, read);
                        cx.notify();
                    }
                })
                .ok();
            })
            .detach();
        }
        if let Some(dir) = f.repo_wanted() {
            cx.spawn(async move |this, cx| {
                let text = |name: &str| {
                    let (client, path) = (client.clone(), child(&dir, name));
                    cx.background_executor().spawn(async move { client.read(&path, 64 * 1024).ok().map(|b| String::from_utf8_lossy(&b).into_owned()) })
                };
                let (head, config) = futures::join!(text(".git/HEAD"), text(".git/config"));
                let repo = Repo { branch: head.as_deref().and_then(branch_of), remote: config.as_deref().and_then(remote_of) };
                this.update(cx, |d, cx| {
                    if let Some(f) = d.opened_as(opened) {
                        f.repo_read(&host, &dir, repo);
                        cx.notify();
                    }
                })
                .ok();
            })
            .detach();
        }
    }

    pub(crate) fn select(&mut self, column: String, name: String, cx: &mut Context<Self>) {
        let Some(f) = self.finder.as_mut() else { return };
        f.select(&column, &name);
        self.fetch(cx);
        cx.notify();
    }

    fn go(&mut self, dir: String, cx: &mut Context<Self>) {
        let Some(f) = self.finder.as_mut() else { return };
        f.go(dir);
        self.fetch(cx);
        cx.notify();
    }

    pub(crate) fn toggle_hidden(&mut self, cx: &mut Context<Self>) {
        if let Some(f) = self.finder.as_mut() {
            f.toggle_hidden();
            cx.notify();
        }
    }

    /// How the footer reads for what is picked now.
    fn finder_ask(&self) -> Option<Ask> {
        let f = self.finder.as_ref()?;
        let taken = self.side.projects.contains(&hosts::place(&f.host, &f.dir));
        let clone = (self.opener == Opener::CloneInto).then_some(self.side.clone.as_deref()).flatten();
        Some(ask(clone, remote::connected(&f.host).is_some(), &f.dir, taken, f.is_repo(&f.dir)))
    }

    /// The picked folder, when the footer allows choosing it.
    fn target(&self) -> Option<(String, String)> {
        let f = self.finder.as_ref()?;
        self.finder_ask()?.ready.then(|| (f.host.clone(), f.dir.clone()))
    }

    pub(crate) fn choose(&mut self, cx: &mut Context<Self>) {
        let Some((host, dir)) = self.target() else { return };
        self.finder = None;
        cx.emit(FinderEvent::Chosen { opener: self.opener, host, dir });
        cx.notify();
    }

    fn on_key(&mut self, ev: &KeyDownEvent, cx: &mut Context<Self>) {
        let key = ev.keystroke.key.as_str();
        if key == "enter" {
            self.choose(cx);
            return cx.stop_propagation();
        }
        if matches!(key, "up" | "down" | "left" | "right") {
            self.step(key, cx);
            cx.stop_propagation();
        }
    }

    /// Moves the pick for an arrow key, keeping it in view.
    pub(crate) fn step(&mut self, key: &str, cx: &mut Context<Self>) {
        let Some(f) = self.finder.as_mut() else { return };
        if !f.step(key) {
            return;
        }
        if let Some((column, at)) = f.cursor() {
            self.scroll(&column).scroll_to_item(at, ScrollStrategy::Center);
        }
        self.fetch(cx);
        cx.notify();
    }

    /// The keys of the projects on `host`.
    fn finder_projects(&self, host: &str) -> Vec<String> {
        self.side.projects.iter().filter(|k| hosts::split(k).0 == Some(host)).cloned().collect()
    }
}

impl Render for HostFinder {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(f) = self.finder.as_ref() else { return div() };
        let (host, error, homed) = (f.host.clone(), f.error.clone(), f.home.is_some());
        let name = self.side.name.clone();
        let online = remote::connected(&host).is_some();
        let max_h = (window.viewport_size().height - px(96.)).min(px(620.));
        let body = match (error, homed) {
            _ if !online => self.finder_note("server", format!("{name} is offline"), "Its folders show once its pocketd answers again."),
            (Some(e), _) => self.finder_note("warning", format!("Couldn\u{2019}t reach {name}"), e),
            (None, false) => self.finder_note("spinner", format!("Connecting to {name}\u{2026}"), ""),
            (None, true) => {
                let projects = self.finder_projects(&host);
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .child(self.finder_places(&projects, cx))
                    .child(self.finder_columns(Rc::new(projects), cx))
                    .child(self.finder_detail())
            }
        };
        let panel = ui::pop(div().w(px(980.)).max_w(window.viewport_size().width - px(48.)).h(max_h).flex().flex_col().overflow_hidden())
            .rounded(px(R_DIALOG))
            .occlude()
            .track_focus(&self.focus)
            .capture_key_down(cx.listener(|this, ev: &KeyDownEvent, _, cx| this.on_key(ev, cx)))
            .child(self.finder_header(&name, online, cx))
            .child(body)
            .child(self.finder_footer(online, cx));
        let backdrop = ui::backdrop("finder-backdrop", 0x2e).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
            this.close(cx);
        }));
        let enter = Animation::new(Duration::from_millis(180)).with_easing(ease_out_quint());
        div().absolute().inset_0().child(backdrop).child(div().absolute().inset_0().flex().items_center().justify_center().child(panel).with_animation(
            "finder-in",
            enter,
            |d, t| d.opacity(t),
        ))
    }
}

impl Desktop {
    /// Opens the finder on `host`, at `start` or the host's home.
    pub(crate) fn open_host_finder(&mut self, host: String, start: Option<String>, opener: Opener, window: &mut Window, cx: &mut Context<Self>) {
        self.host_finder.update(cx, |f, cx| f.open(host, start, opener, window, cx));
        cx.notify();
    }

    /// Closes the finder, leaving what opened it as it was. False when it wasn't open.
    pub(crate) fn close_host_finder(&mut self, cx: &mut Context<Self>) -> bool {
        self.host_finder.update(cx, |f, cx| f.close(cx))
    }

    pub(crate) fn on_finder_event(&mut self, _: &Entity<HostFinder>, ev: &FinderEvent, window: &mut Window, cx: &mut Context<Self>) {
        match ev {
            FinderEvent::Closed(opener) => self.finder_closed(*opener, window, cx),
            FinderEvent::Chosen { opener, host, dir } => {
                self.finder_closed(*opener, window, cx);
                let (host, dir) = (host.clone(), dir.clone());
                match opener {
                    Opener::AddProject => self.repo_form.update(cx, |f, cx| f.pick_host_folder(host, dir, window, cx)),
                    Opener::NewSession => self.pick_new_project(hosts::place(&host, &dir), window, cx),
                    Opener::Sidebar => self.open_chosen_folder(hosts::place(&host, &dir), window, cx),
                    Opener::CloneInto => self.clone_into(host, dir, window, cx),
                }
                cx.notify();
            }
        }
    }

    fn finder_closed(&mut self, opener: Opener, window: &mut Window, cx: &mut Context<Self>) {
        match opener {
            Opener::AddProject | Opener::Sidebar => window.focus(&self.root, cx),
            Opener::NewSession => self.focus_prompt(window, cx),
            Opener::CloneInto => self.clone_form.read(cx).url.clone().update(cx, |s, cx| s.focus(window, cx)),
        }
    }

    pub(crate) fn host_choices(&self) -> Vec<HostRow> {
        self.fleet
            .hosts
            .list
            .iter()
            .map(|h| HostRow { id: h.id.clone(), name: self.host_name(&h.id), glyph: self.space_glyph(&Space::Host(h.id.clone())) })
            .collect()
    }

    /// The finder, when it's open, over what `Desktop` hands it.
    pub(crate) fn host_finder_view(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let finder = self.host_finder.read(cx);
        let (host, opener) = (finder.host()?.to_string(), finder.opener());
        let side = Side {
            name: self.host_name(&host),
            glyph: self.space_glyph(&Space::Host(host)),
            hosts: self.host_choices(),
            projects: self.projects(),
            clone: (opener == Opener::CloneInto).then(|| self.clone_form.read(cx).repo_name(cx)),
        };
        self.host_finder.update(cx, |f, _| f.hand(side));
        Some(div().absolute().inset_0().child(self.host_finder.clone()).into_any_element())
    }
}
