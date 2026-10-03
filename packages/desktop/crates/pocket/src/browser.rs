pub(crate) mod view;

use crate::actions::{Back, FocusAddress, Forward, NewBrowser, PageEdit, Reload};
use crate::desktop::Desktop;
use crate::desktop::chrome::{Confirm, Layout, Overlay, Screen};
use futures::StreamExt;
use futures::channel::mpsc::{UnboundedSender, unbounded};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::*;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use workspace::Tab;

pub(crate) const CONTEXT: &str = "Browser";

#[derive(Default)]
pub(crate) struct Browser {
    /// Built on the first load, so an empty tab costs no WebKit process.
    pub(crate) page: Option<Rc<web::Page>>,
    pub(crate) url: String,
    pub(crate) title: String,
    pub(crate) loading: bool,
}

impl Browser {
    pub(crate) fn label(&self) -> &str {
        [self.title.as_str(), web::host(&self.url)].into_iter().find(|t| !t.is_empty()).unwrap_or("New tab")
    }
}

pub struct Browsers {
    pub(crate) tabs: HashMap<u64, Browser>,
    next: u64,
    /// One address bar, shared by every tab since only one is on screen.
    pub(crate) address: Entity<InputState>,
    /// Held while the page has the keys, so GPUI's keymap still sees ⌘-keys in the "Browser" context.
    pub(crate) focus: FocusHandle,
    /// The tab whose page may be placed: a page draws over GPUI, so it is parked under any menu or sheet.
    pub(crate) shown: Option<u64>,
    events: UnboundedSender<(u64, web::Event)>,
}

impl Browsers {
    pub fn new(window: &mut Window, cx: &mut Context<Desktop>) -> (Self, Vec<Subscription>) {
        let address = cx.new(|cx| InputState::new(window, cx).placeholder("Search or enter address"));
        let subs = vec![cx.subscribe_in(&address, window, |this, address, ev: &InputEvent, window, cx| {
            let text = address.read(cx).value();
            if let InputEvent::PressEnter { secondary: false, .. } = ev
                && !text.trim().is_empty()
                && let Some(id) = this.active_web_tab()
            {
                this.browse(id, web::resolve(&text), window, cx);
            }
        })];
        let (events, mut rx) = unbounded();
        cx.spawn_in(window, async move |this, cx| {
            while let Some((id, ev)) = rx.next().await {
                if this.update_in(cx, |d: &mut Desktop, window, cx| d.on_page(id, ev, window, cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
        (Self { tabs: HashMap::new(), next: 0, address, focus: cx.focus_handle(), shown: None, events }, subs)
    }

    fn open(&mut self) -> u64 {
        let id = self.next;
        self.next += 1;
        self.tabs.insert(id, Browser::default());
        id
    }

    /// Drops the tabs no workspace holds any more, and parks every page but `shown`'s.
    fn show(&mut self, shown: Option<u64>, live: &HashSet<u64>) {
        self.tabs.retain(|id, _| live.contains(id));
        for (_, b) in self.tabs.iter().filter(|(id, _)| Some(**id) != shown) {
            if let Some(page) = &b.page {
                page.park();
            }
        }
        self.shown = shown;
    }

    fn page(&self) -> Option<&web::Page> {
        self.tabs.get(&self.shown?)?.page.as_deref()
    }
}

impl Desktop {
    pub(crate) fn active_web_tab(&mut self) -> Option<u64> {
        let tree = self.session_tree()?;
        match self.workspace(&tree).active()? {
            Tab::Web(id) => Some(*id),
            _ => None,
        }
    }

    pub(crate) fn sync_browser(&mut self) {
        if self.browsers.tabs.is_empty() {
            return;
        }
        let covered = self.overlay.is_some() || self.menu_open() || self.sidebar.menu_at.is_some() || (self.layout == Layout::Compact && self.panel) || self.shown_create().is_some();
        let shown = self.active_web_tab().filter(|_| !covered);
        let live = self.workspaces.values().flat_map(|w| &w.tabs).filter_map(|t| if let Tab::Web(id) = t { Some(*id) } else { None }).collect();
        self.browsers.show(shown, &live);
    }

    /// Opens a browser tab in the worktree on screen, at `url` or with the address bar focused.
    pub(crate) fn open_browser(&mut self, url: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tree) = self.cwd() else { return };
        let id = self.browsers.open();
        self.workspace(&tree).open_web(id);
        self.screen = Screen::Sessions;
        match url {
            Some(url) => self.browse(id, url, window, cx),
            None => self.focus_address(&FocusAddress, window, cx),
        }
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
            web::Event::Pressed => window.focus(&self.browsers.focus, cx),
        }
        cx.notify();
    }

    pub(crate) fn new_browser(&mut self, _: &NewBrowser, window: &mut Window, cx: &mut Context<Self>) {
        self.close_menus();
        self.open_browser(None, window, cx);
    }

    fn focus_address(&mut self, _: &FocusAddress, window: &mut Window, cx: &mut Context<Self>) {
        let Some(b) = self.active_web_tab().and_then(|id| self.browsers.tabs.get(&id)) else { return };
        if let Some(page) = &b.page {
            page.give_keys();
        }
        let url = b.url.clone();
        self.browsers.address.update(cx, |s, cx| {
            s.set_value(url, window, cx);
            s.focus(window, cx);
            s.select_all(window, cx);
        });
    }

    fn reload(&mut self, _: &Reload, _: &mut Window, _: &mut Context<Self>) {
        if let Some(page) = self.browsers.page() {
            page.reload();
        }
    }

    fn back(&mut self, _: &Back, _: &mut Window, _: &mut Context<Self>) {
        if let Some(page) = self.browsers.page() {
            page.back();
        }
    }

    fn forward(&mut self, _: &Forward, _: &mut Window, _: &mut Context<Self>) {
        if let Some(page) = self.browsers.page() {
            page.forward();
        }
    }

    fn page_edit(&mut self, edit: &PageEdit, _: &mut Window, _: &mut Context<Self>) {
        if let Some(page) = self.browsers.page() {
            page.edit(edit.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Browser;

    #[test]
    fn a_tab_is_named_by_its_page_title_then_its_host() {
        let mut b = Browser { url: "https://example.com/docs?q=1".into(), ..Browser::default() };
        assert_eq!(b.label(), "example.com");
        b.title = "Example Docs".into();
        assert_eq!(b.label(), "Example Docs");
        assert_eq!(Browser::default().label(), "New tab");
    }
}
