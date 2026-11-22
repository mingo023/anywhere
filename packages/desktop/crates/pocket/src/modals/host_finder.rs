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

pub(crate) struct HostFinder {
    pub(crate) finder: Option<Finder>,
    opener: Opener,
    focus: FocusHandle,
    scrolls: HashMap<String, UniformListScrollHandle>,
    preview_scroll: UniformListScrollHandle,
    host_menu: bool,
    opened: usize,
}

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
        }
    }

    fn scroll(&mut self, dir: &str) -> UniformListScrollHandle {
        self.scrolls.entry(dir.to_string()).or_default().clone()
    }

    /// The finder, if it's still the one opened `opened`th: one closed and opened again on the same host takes none of the first one's replies.
    fn opened_as(&mut self, opened: usize) -> Option<&mut Finder> {
        let same = self.opened == opened;
        self.finder.as_mut().filter(|_| same)
    }
}

impl Desktop {
    /// Opens the finder on `host`, at `start` or the host's home.
    pub(crate) fn open_host_finder(&mut self, host: String, start: Option<String>, opener: Opener, window: &mut Window, cx: &mut Context<Self>) {
        let hf = &mut self.host_finder;
        hf.finder = Some(Finder::new(&host));
        hf.opener = opener;
        hf.host_menu = false;
        hf.scrolls.clear();
        hf.opened += 1;
        let opened = hf.opened;
        window.focus(&hf.focus, cx);
        cx.notify();
        let Some(client) = remote::connected(&host) else {
            if let Some(f) = self.host_finder.finder.as_mut() {
                f.error = Some("Not connected to this host. Reconnect it, then open the folder again.".into());
            }
            return;
        };
        cx.spawn(async move |this, cx| {
            let home = cx.background_executor().spawn(async move { client.home() }).await;
            this.update(cx, |d, cx| {
                let Some(f) = d.host_finder.opened_as(opened) else { return };
                match home {
                    Ok(home) => f.homed(home, start),
                    Err(e) => f.error = Some(e.to_string()),
                }
                d.finder_fetch(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Opens again a finder that was left without `host`'s home, now that the host answers.
    pub(crate) fn finder_host_back(&mut self, host: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.host_finder.finder.as_ref().is_some_and(|f| f.host == host && f.home.is_none()) {
            self.open_host_finder(host.into(), None, self.host_finder.opener, window, cx);
        }
    }

    /// Closes the finder, leaving what opened it as it was. False when it wasn't open.
    pub(crate) fn close_host_finder(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.host_finder.finder.take().is_none() {
            return false;
        }
        match self.host_finder.opener {
            Opener::AddProject | Opener::Sidebar => window.focus(&self.root, cx),
            Opener::NewSession => self.focus_prompt(window, cx),
            Opener::CloneInto => self.clone_form.read(cx).url.clone().update(cx, |s, cx| s.focus(window, cx)),
        }
        true
    }

    /// Asks the host, off the UI thread, for whatever the finder now shows but hasn't got.
    fn finder_fetch(&mut self, cx: &mut Context<Self>) {
        let Some(f) = self.host_finder.finder.as_mut() else { return };
        let Some(client) = remote::connected(&f.host) else { return };
        let (host, opened) = (f.host.clone(), self.host_finder.opened);
        for dir in f.wanted() {
            let (client, host) = (client.clone(), host.clone());
            cx.spawn(async move |this, cx| {
                let asked = dir.clone();
                let listed = cx.background_executor().spawn(async move { client.list(&asked).map_err(|e| e.to_string()) }).await;
                this.update(cx, |d, cx| {
                    if d.host_finder.opened_as(opened).is_some_and(|f| f.listed(&host, &dir, listed)) {
                        d.finder_fetch(cx);
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
                    if let Some(f) = d.host_finder.opened_as(opened) {
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
                    if let Some(f) = d.host_finder.opened_as(opened) {
                        f.repo_read(&host, &dir, repo);
                        cx.notify();
                    }
                })
                .ok();
            })
            .detach();
        }
    }

    pub(crate) fn finder_select(&mut self, column: String, name: String, cx: &mut Context<Self>) {
        let Some(f) = self.host_finder.finder.as_mut() else { return };
        f.select(&column, &name);
        self.finder_fetch(cx);
        cx.notify();
    }

    fn finder_go(&mut self, dir: String, cx: &mut Context<Self>) {
        let Some(f) = self.host_finder.finder.as_mut() else { return };
        f.go(dir);
        self.finder_fetch(cx);
        cx.notify();
    }

    pub(crate) fn finder_toggle_hidden(&mut self, cx: &mut Context<Self>) {
        if let Some(f) = self.host_finder.finder.as_mut() {
            f.toggle_hidden();
            cx.notify();
        }
    }

    /// How the footer reads for what is picked now.
    fn finder_ask(&self, cx: &App) -> Option<Ask> {
        let f = self.host_finder.finder.as_ref()?;
        let taken = self.projects().contains(&hosts::place(&f.host, &f.dir));
        let clone = (self.host_finder.opener == Opener::CloneInto).then(|| self.clone_form.read(cx).repo_name(cx));
        Some(ask(clone.as_deref(), remote::connected(&f.host).is_some(), &f.dir, taken, f.is_repo(&f.dir)))
    }

    /// The picked folder, when the footer allows choosing it.
    fn finder_target(&self, cx: &App) -> Option<(String, String)> {
        let f = self.host_finder.finder.as_ref()?;
        self.finder_ask(cx)?.ready.then(|| (f.host.clone(), f.dir.clone()))
    }

    pub(crate) fn finder_choose(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((host, dir)) = self.finder_target(cx) else { return };
        self.close_host_finder(window, cx);
        match self.host_finder.opener {
            Opener::AddProject => self.pick_host_folder(host, dir, window, cx),
            Opener::NewSession => self.pick_new_project(hosts::place(&host, &dir), window, cx),
            Opener::Sidebar => self.open_chosen_folder(hosts::place(&host, &dir), window, cx),
            Opener::CloneInto => self.clone_into(host, dir, window, cx),
        }
        cx.notify();
    }

    fn on_finder_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = ev.keystroke.key.as_str();
        if key == "enter" {
            self.finder_choose(window, cx);
            return cx.stop_propagation();
        }
        if matches!(key, "up" | "down" | "left" | "right") {
            self.finder_step(key, cx);
            cx.stop_propagation();
        }
    }

    /// Moves the pick for an arrow key, keeping it in view.
    pub(crate) fn finder_step(&mut self, key: &str, cx: &mut Context<Self>) {
        let hf = &mut self.host_finder;
        let Some(f) = hf.finder.as_mut() else { return };
        if !f.step(key) {
            return;
        }
        if let Some((column, at)) = f.cursor() {
            hf.scroll(&column).scroll_to_item(at, ScrollStrategy::Center);
        }
        self.finder_fetch(cx);
        cx.notify();
    }

    /// The keys of the projects on `host`.
    fn finder_projects(&self, host: &str) -> Vec<String> {
        self.projects().into_iter().filter(|k| hosts::split(k).0 == Some(host)).collect()
    }

    /// The finder, when it's open.
    pub(crate) fn host_finder_view(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let f = self.host_finder.finder.as_ref()?;
        let (host, error, homed) = (f.host.clone(), f.error.clone(), f.home.is_some());
        let name = self.host_name(&host);
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
            .track_focus(&self.host_finder.focus)
            .capture_key_down(cx.listener(Self::on_finder_key))
            .child(self.finder_header(&name, online, cx))
            .child(body)
            .child(self.finder_footer(online, cx));
        let backdrop = ui::backdrop("finder-backdrop", 0x2e).on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
            this.close_host_finder(window, cx);
        }));
        let enter = Animation::new(Duration::from_millis(180)).with_easing(ease_out_quint());
        Some(
            div()
                .absolute()
                .inset_0()
                .child(backdrop)
                .child(div().absolute().inset_0().flex().items_center().justify_center().child(panel).with_animation("finder-in", enter, |d, t| d.opacity(t)))
                .into_any_element(),
        )
    }
}
