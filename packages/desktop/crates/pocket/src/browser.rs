pub(crate) mod view;

use crate::actions::{Back, FocusAddress, Forward, NewBrowser, PageEdit, Reload};
use crate::desktop::Desktop;
use crate::desktop::chrome::{Confirm, Overlay, Screen};
use futures::StreamExt;
use futures::channel::mpsc::{UnboundedSender, unbounded};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::*;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;
use workspace::Tab;
use workspace::tree::PaneId;

pub(crate) const CONTEXT: &str = "Browser";

#[derive(Default)]
pub(crate) struct Browser {
    /// Built on the first load, so an empty tab costs no WebKit process.
    pub(crate) page: Option<Rc<web::Page>>,
    pub(crate) url: String,
    pub(crate) title: String,
    pub(crate) loading: bool,
    /// What the page showed when a tab was picked up; drawn in its place while the page is parked for the drag.
    pub(crate) shot: Option<Arc<RenderImage>>,
}

impl Browser {
    pub(crate) fn label(&self) -> &str {
        [self.title.as_str(), web::host(&self.url)].into_iter().find(|t| !t.is_empty()).unwrap_or("New tab")
    }
}

pub struct Browsers {
    pub(crate) tabs: HashMap<u64, Browser>,
    next: u64,
    /// Each pane's address bar, built the first time it shows a tab, with the subscription that browses from it.
    addresses: HashMap<PaneId, (Entity<InputState>, Subscription)>,
    /// Held while the page has the keys, so GPUI's keymap still sees ⌘-keys in the "Browser" context.
    pub(crate) focus: FocusHandle,
    /// The tabs whose pages may be placed: a page draws over GPUI, so it is parked under any menu or sheet.
    pub(crate) shown: HashSet<u64>,
    events: UnboundedSender<(u64, web::Event)>,
    shots: UnboundedSender<(u64, Option<Vec<u8>>)>,
}

impl Browsers {
    pub fn new(window: &mut Window, cx: &mut Context<Desktop>) -> Self {
        let (events, mut rx) = unbounded();
        cx.spawn_in(window, async move |this, cx| {
            while let Some((id, ev)) = rx.next().await {
                if this.update_in(cx, |d: &mut Desktop, window, cx| d.on_page(id, ev, window, cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
        let (shots, mut shots_rx) = unbounded::<(u64, Option<Vec<u8>>)>();
        cx.spawn_in(window, async move |this, cx| {
            while let Some((id, tiff)) = shots_rx.next().await {
                let image = cx.background_spawn(async move { tiff.and_then(|t| shot(&t)) }).await;
                let applied = this.update_in(cx, |d: &mut Desktop, window, cx| {
                    // A shot that lands after the drop would cover the live page.
                    if d.panels.drag.is_some()
                        && let Some(b) = d.browsers.tabs.get_mut(&id)
                    {
                        if let Some(old) = std::mem::replace(&mut b.shot, image) {
                            window.drop_image(old).ok();
                        }
                        cx.notify();
                    }
                });
                if applied.is_err() {
                    break;
                }
            }
        })
        .detach();
        Self { tabs: HashMap::new(), next: 0, addresses: HashMap::new(), focus: cx.focus_handle(), shown: HashSet::new(), events, shots }
    }

    fn address(&mut self, pane: PaneId, window: &mut Window, cx: &mut Context<Desktop>) -> Entity<InputState> {
        let (address, _) = self.addresses.entry(pane).or_insert_with(|| {
            let address = cx.new(|cx| InputState::new(window, cx).placeholder("Search or enter address"));
            let sub = cx.subscribe_in(&address, window, move |this, address, ev: &InputEvent, window, cx| {
                let text = address.read(cx).value();
                if let InputEvent::PressEnter { secondary: false, .. } = ev
                    && !text.trim().is_empty()
                    && let Some(id) = this.web_tab(pane)
                {
                    this.browse(id, web::resolve(&text, this.store.browser.search_url()), window, cx);
                }
            });
            (address, sub)
        });
        address.clone()
    }

    fn open(&mut self) -> u64 {
        let id = self.next;
        self.next += 1;
        self.tabs.insert(id, Browser::default());
        id
    }

    /// Drops the tabs no workspace holds any more, and parks every page but `shown`'s.
    fn show(&mut self, shown: &[u64], live: &HashSet<u64>) {
        self.tabs.retain(|id, _| live.contains(id));
        for (_, b) in self.tabs.iter().filter(|(id, _)| !shown.contains(id)) {
            if let Some(page) = &b.page {
                page.park();
            }
        }
        self.shown = shown.iter().copied().collect();
    }

    fn page(&self, id: u64) -> Option<&web::Page> {
        self.tabs.get(&id).filter(|_| self.shown.contains(&id))?.page.as_deref()
    }

    /// Drops the address bars of panes that closed.
    pub fn retain_panes(&mut self, panes: &[PaneId]) {
        self.addresses.retain(|p, _| panes.contains(p));
    }
}

impl Desktop {
    pub(crate) fn web_tab(&mut self, pane: PaneId) -> Option<u64> {
        self.session_tree()?;
        match self.pane_tab(pane)? {
            Tab::Web(id) => Some(id),
            _ => None,
        }
    }

    fn focused_page(&mut self) -> Option<&web::Page> {
        let id = self.web_tab(self.focused_pane())?;
        self.browsers.page(id)
    }

    pub(crate) fn sync_browser(&mut self, window: &mut Window) {
        if self.browsers.tabs.is_empty() {
            return;
        }
        let covered = self.overlay.is_some() || self.menu_open() || self.sidebar.menu_at.is_some() || self.shown_create().is_some() || self.panels.drag.as_ref().is_some_and(|(_, d)| d.away);
        let shown: Vec<u64> = match self.session_tree().and_then(|t| self.workspaces.get(&t)) {
            Some(w) if !covered => w.shown().into_iter().filter_map(|(_, t)| web(t)).collect(),
            _ => Vec::new(),
        };
        let live: HashSet<u64> = self.workspaces.values().flat_map(|w| w.tree.panes()).flat_map(|p| &p.tabs).filter_map(web).collect();
        let ended = self.panels.drag.is_none();
        for shot in self.browsers.tabs.iter_mut().filter(|(id, _)| ended || !live.contains(*id)).filter_map(|(_, b)| b.shot.take()) {
            window.drop_image(shot).ok();
        }
        self.browsers.show(&shown, &live);
    }

    /// Takes the keys back from the focused pane's page, which a move to the pane beside it leaves placed and so first responder.
    pub(crate) fn release_page(&mut self) {
        if let Some(page) = self.focused_page() {
            page.give_keys();
        }
    }

    /// Asks each shown page for a picture of itself, to draw while a held tab parks the pages.
    pub(crate) fn freeze_pages(&mut self, window: &mut Window) {
        let Some(w) = self.session_tree().and_then(|t| self.workspaces.get(&t)) else { return };
        let shown: Vec<u64> = w.shown().into_iter().filter_map(|(_, t)| web(t)).collect();
        for id in shown {
            let Some(b) = self.browsers.tabs.get_mut(&id) else { continue };
            if let Some(old) = b.shot.take() {
                window.drop_image(old).ok();
            }
            let Some(page) = &b.page else { continue };
            let shots = self.browsers.shots.clone();
            page.snapshot(move |tiff| {
                let _ = shots.unbounded_send((id, tiff));
            });
        }
    }

    /// Opens a browser tab in the worktree on screen, at `url` or with the address bar focused.
    pub(crate) fn open_browser(&mut self, url: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tree) = self.cwd() else { return };
        let dev_url = self.project.as_ref().and_then(|p| self.store.repos.get(p)).map(|r| r.dev_url.as_str()).unwrap_or_default();
        let url = url.or_else(|| self.store.browser.home_page(dev_url).map(|u| web::resolve(&u, self.store.browser.search_url())));
        let id = self.browsers.open();
        self.workspace(&tree).open_web(id);
        self.screen = Screen::Sessions;
        match url {
            Some(url) => self.browse(id, url, window, cx),
            None => self.focus_address(&FocusAddress, window, cx),
        }
        self.save_soon(cx);
        cx.notify();
    }

    fn browse(&mut self, id: u64, url: String, window: &mut Window, cx: &mut Context<Self>) {
        let events = self.browsers.events.clone();
        let Some(b) = self.browsers.tabs.get_mut(&id) else { return };
        b.url = url.clone();
        match &b.page {
            Some(page) => {
                page.load(&url);
                page.take_keys();
            }
            None => match web::Page::new(&url, &*window, move |ev| {
                let _ = events.unbounded_send((id, ev));
            }) {
                Ok(page) => b.page = Some(Rc::new(page)),
                Err(e) => self.error = Some(format!("Couldn't open the page: {e}")),
            },
        }
        window.focus(&self.browsers.focus, cx);
        cx.notify();
    }

    fn on_page(&mut self, id: u64, ev: web::Event, window: &mut Window, cx: &mut Context<Self>) {
        let Some(b) = self.browsers.tabs.get_mut(&id) else { return };
        match ev {
            web::Event::Started(url) => (b.loading, b.url) = (true, url),
            web::Event::Finished(url) => (b.loading, b.url) = (false, url),
            web::Event::Title(title) => b.title = title,
            web::Event::Moved => {
                if let Some(url) = b.page.as_ref().and_then(|p| p.url()) {
                    b.url = url;
                }
            }
            web::Event::Open(url) => self.open_browser(Some(url), window, cx),
            // wry reports frame and script navigations too, so a page could launch apps unasked, or swap out a sheet the user is answering.
            web::Event::External(url) => {
                if self.overlay.is_none() {
                    self.confirm = Some(Confirm::OpenExternal(url));
                    self.open(Overlay::Confirm, window, cx);
                }
            }
            web::Event::Pressed => {
                if let Some(w) = self.cwd().and_then(|t| self.workspaces.get_mut(&t))
                    && let Some((pane, _)) = w.tree.find(|t| *t == Tab::Web(id))
                    && w.tree.focused != pane
                {
                    w.tree.focus(pane);
                    self.save_soon(cx);
                }
                window.focus(&self.browsers.focus, cx);
            }
        }
        cx.notify();
    }

    pub(crate) fn new_browser(&mut self, _: &NewBrowser, window: &mut Window, cx: &mut Context<Self>) {
        self.close_menus();
        self.open_browser(None, window, cx);
    }

    fn focus_address(&mut self, _: &FocusAddress, window: &mut Window, cx: &mut Context<Self>) {
        let pane = self.focused_pane();
        let Some(b) = self.web_tab(pane).and_then(|id| self.browsers.tabs.get(&id)) else { return };
        if let Some(page) = &b.page {
            page.give_keys();
        }
        let url = b.url.clone();
        self.browsers.address(pane, window, cx).update(cx, |s, cx| {
            s.set_value(url, window, cx);
            s.focus(window, cx);
            s.select_all(window, cx);
        });
    }

    fn reload(&mut self, _: &Reload, _: &mut Window, _: &mut Context<Self>) {
        if let Some(page) = self.focused_page() {
            page.reload();
        }
    }

    fn back(&mut self, _: &Back, _: &mut Window, _: &mut Context<Self>) {
        if let Some(page) = self.focused_page() {
            page.back();
        }
    }

    fn forward(&mut self, _: &Forward, _: &mut Window, _: &mut Context<Self>) {
        if let Some(page) = self.focused_page() {
            page.forward();
        }
    }

    fn page_edit(&mut self, edit: &PageEdit, _: &mut Window, _: &mut Context<Self>) {
        if let Some(page) = self.focused_page() {
            page.edit(edit.0);
        }
    }
}

fn web(t: &Tab) -> Option<u64> {
    if let Tab::Web(id) = t { Some(*id) } else { None }
}

/// Decodes a page snapshot into the BGRA image GPUI draws.
fn shot(tiff: &[u8]) -> Option<Arc<RenderImage>> {
    let rgba = image::load_from_memory_with_format(tiff, image::ImageFormat::Tiff).ok()?.into_rgba8();
    let (w, h) = rgba.dimensions();
    let mut bgra = rgba.into_raw();
    for px in bgra.as_chunks_mut::<4>().0 {
        px.swap(0, 2);
    }
    let buffer = image::ImageBuffer::from_raw(w, h, bgra)?;
    Some(Arc::new(RenderImage::new(vec![image::Frame::new(buffer)])))
}

#[cfg(test)]
mod tests {
    use super::{Browser, shot};
    use std::io::Cursor;

    #[test]
    fn a_tab_is_named_by_its_page_title_then_its_host() {
        let mut b = Browser { url: "https://example.com/docs?q=1".into(), ..Browser::default() };
        assert_eq!(b.label(), "example.com");
        b.title = "Example Docs".into();
        assert_eq!(b.label(), "Example Docs");
        assert_eq!(Browser::default().label(), "New tab");
    }

    #[test]
    fn a_page_snapshot_decodes_to_the_bgra_gpui_draws() {
        let rgba = image::RgbaImage::from_raw(2, 1, vec![10, 20, 30, 255, 40, 50, 60, 128]).unwrap();
        let mut tiff = Cursor::new(Vec::new());
        rgba.write_to(&mut tiff, image::ImageFormat::Tiff).unwrap();
        let image = shot(tiff.get_ref()).unwrap();
        assert_eq!(image.as_bytes(0), Some(&[30, 20, 10, 255, 60, 50, 40, 128][..]));
    }
}
