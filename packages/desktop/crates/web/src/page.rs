use std::cell::Cell;
use std::rc::Rc;
#[cfg(not(target_os = "macos"))]
use wry::dpi::{LogicalPosition, LogicalSize};
use wry::raw_window_handle::HasWindowHandle;
use wry::{NewWindowResponse, PageLoadEvent, WebView, WebViewBuilder};

#[cfg(target_os = "macos")]
mod macos;

/// A rect in the parent window, in logical pixels from its top left.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug)]
pub enum Event {
    Started(String),
    Finished(String),
    Title(String),
    /// The page moved its own history, which fires no load; read the URL back with [`Page::url`].
    Moved,
    /// A link or script asked for a window of its own.
    Open(String),
    /// A link to a scheme another app handles.
    External(String),
    Pressed,
}

/// Edits WKWebView takes only as responder actions: without an Edit menu, ⌘C in the page copies nothing.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Edit {
    Copy,
    Cut,
    Paste,
    SelectAll,
    Undo,
    Redo,
}

/// Hidden alone, a WKWebView still takes every drag that crosses its last rect, so a parked page also moves out of the window.
const PARKED: f32 = -20_000.;

/// Run in the main frame, where wry defines `window.ipc`.
const MOVED: &str = "(() => {
    const moved = () => window.ipc.postMessage('moved');
    for (const name of ['pushState', 'replaceState']) {
        const original = history[name];
        history[name] = function (...args) {
            const result = original.apply(this, args);
            moved();
            return result;
        };
    }
    addEventListener('popstate', moved);
    addEventListener('hashchange', moved);
})();";

const SCHEMES: [&str; 7] = ["http:", "https:", "about:", "data:", "blob:", "file:", "javascript:"];

/// A WKWebView laid over the parent window's view, parked until placed.
pub struct Page {
    view: WebView,
    placed: Cell<Option<Rect>>,
    #[cfg(target_os = "macos")]
    glue: macos::Glue,
}

impl Page {
    /// `sink` runs on the main thread, from inside WebKit's callbacks.
    pub fn new(url: &str, parent: &impl HasWindowHandle, sink: impl Fn(Event) + 'static) -> wry::Result<Self> {
        let sink = Rc::new(sink);
        let [ipc, loads, titles, opens, links] = [(); 5].map(|_| sink.clone());
        let parked = Rect { x: PARKED, y: PARKED, width: 800., height: 600. };
        let builder = WebViewBuilder::new()
            .with_url(url)
            .with_initialization_script(MOVED)
            .with_ipc_handler(move |request| {
                if request.body() == "moved" {
                    ipc(Event::Moved);
                }
            })
            .with_on_page_load_handler(move |event, url| {
                loads(match event {
                    PageLoadEvent::Started => Event::Started(url),
                    PageLoadEvent::Finished => Event::Finished(url),
                })
            })
            .with_document_title_changed_handler(move |title| titles(Event::Title(title)))
            .with_new_window_req_handler(move |url, _| {
                opens(if opens_here(&url) { Event::Open(url) } else { Event::External(url) });
                NewWindowResponse::Deny
            })
            .with_navigation_handler(move |url| {
                let here = opens_here(&url);
                if !here {
                    links(Event::External(url));
                }
                here
            })
            .with_download_started_handler(|_, _| true)
            .with_back_forward_navigation_gestures(true)
            .with_devtools(true);
        #[cfg(target_os = "macos")]
        {
            let frame = macos::Frame::new(parent, parked)?;
            let view = builder.with_user_agent(macos::user_agent()).build_as_child(&frame)?;
            frame.fill(&view);
            Ok(Self { view, placed: Cell::new(None), glue: macos::Glue::new(frame, move || sink(Event::Pressed)) })
        }
        #[cfg(not(target_os = "macos"))]
        {
            let view = builder.with_bounds(bounds(parked)).with_visible(false).build_as_child(parent)?;
            Ok(Self { view, placed: Cell::new(None) })
        }
    }

    /// Shows the page over `rect`. True if it was parked, and so doesn't hold the keys.
    pub fn place(&self, rect: Rect) -> bool {
        let was = self.placed.replace(Some(rect));
        if was != Some(rect) && !self.closed() {
            self.set_bounds(rect);
        }
        if was.is_none() {
            self.set_visible(true);
        }
        was.is_none()
    }

    pub fn park(&self) {
        let Some(rect) = self.placed.take() else { return };
        self.give_keys();
        #[cfg(target_os = "macos")]
        self.glue.parked();
        self.set_visible(false);
        if !self.closed() {
            // At its own size, so the page doesn't lay out again for a viewport nobody sees.
            self.set_bounds(Rect { x: PARKED, y: PARKED, ..rect });
        }
    }

    fn set_bounds(&self, rect: Rect) {
        #[cfg(target_os = "macos")]
        self.glue.frame.place(rect);
        #[cfg(not(target_os = "macos"))]
        let _ = self.view.set_bounds(bounds(rect));
    }

    fn set_visible(&self, visible: bool) {
        #[cfg(target_os = "macos")]
        self.glue.frame.set_visible(visible);
        #[cfg(not(target_os = "macos"))]
        let _ = self.view.set_visible(visible);
    }

    pub fn load(&self, url: &str) {
        let _ = self.view.load_url(url);
    }

    pub fn reload(&self) {
        let _ = self.view.reload();
    }

    pub fn back(&self) {
        let _ = self.view.go_back();
    }

    pub fn forward(&self) {
        let _ = self.view.go_forward();
    }

    pub fn can_go_back(&self) -> bool {
        self.view.can_go_back().unwrap_or(false)
    }

    pub fn can_go_forward(&self) -> bool {
        self.view.can_go_forward().unwrap_or(false)
    }

    pub fn url(&self) -> Option<String> {
        self.view.url().ok()
    }

    pub fn take_keys(&self) {
        if self.placed.get().is_some() && !self.closed() {
            let _ = self.view.focus();
        }
    }

    pub fn give_keys(&self) {
        if self.holds_keys() {
            #[cfg(target_os = "macos")]
            self.glue.give_keys();
            #[cfg(not(target_os = "macos"))]
            let _ = self.view.focus_parent();
        }
    }

    /// Whether the page, or a view inside it, is first responder.
    fn holds_keys(&self) -> bool {
        #[cfg(target_os = "macos")]
        return self.glue.holds_keys();
        #[cfg(not(target_os = "macos"))]
        false
    }

    pub fn edit(&self, edit: Edit) {
        #[cfg(target_os = "macos")]
        if self.holds_keys() {
            macos::edit(edit);
        }
        #[cfg(not(target_os = "macos"))]
        let _ = edit;
    }

    /// wry's `set_bounds` and `focus` unwrap the page's window, gone once it closes.
    fn closed(&self) -> bool {
        #[cfg(target_os = "macos")]
        return macos::closed(&self.view);
        #[cfg(not(target_os = "macos"))]
        false
    }
}

/// Removed while first responder, the page would leave the window with none, and GPUI's view deaf to keys.
impl Drop for Page {
    fn drop(&mut self) {
        self.give_keys();
    }
}

fn opens_here(url: &str) -> bool {
    SCHEMES.iter().any(|s| url.starts_with(s))
}

#[cfg(not(target_os = "macos"))]
fn bounds(rect: Rect) -> wry::Rect {
    wry::Rect {
        position: LogicalPosition::new(rect.x, rect.y).into(),
        size: LogicalSize::new(rect.width, rect.height).into(),
    }
}

#[cfg(test)]
mod tests {
    use super::opens_here;

    #[test]
    fn web_pages_open_in_the_tab_and_other_schemes_go_to_their_apps() {
        for url in ["https://example.com", "http://127.0.0.1:3000", "about:blank", "data:text/html,hi", "blob:https://example.com/1"] {
            assert!(opens_here(url), "{url}");
        }
        for url in ["mailto:a@example.com", "zoommtg://zoom.us/join", "vscode://file/tmp", "tel:123"] {
            assert!(!opens_here(url), "{url}");
        }
    }
}
