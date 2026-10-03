mod cursor;
mod press;
mod snapshot;

pub(super) use snapshot::snapshot;

use super::{Edit, Rect};
use objc2::rc::Retained;
use objc2::{MainThreadMarker, MainThreadOnly, sel};
use objc2_app_kit::{NSApplication, NSAutoresizingMaskOptions, NSView};
use objc2_foundation::{NSBundle, NSPoint, NSRect, NSSize, NSString};
use std::ptr::NonNull;
use std::sync::OnceLock;
use wry::raw_window_handle::{AppKitWindowHandle, HandleError, HasWindowHandle, RawWindowHandle, WindowHandle};
use wry::{WebView, WebViewExtMacOS};

/// WebKit docks the inspector across the page's whole superview, so the page gets a view of its own rather than GPUI's.
pub(super) struct Frame(Retained<NSView>);

impl Frame {
    pub(super) fn new(parent: &impl HasWindowHandle, rect: Rect) -> wry::Result<Self> {
        let RawWindowHandle::AppKit(handle) = parent.window_handle()?.as_raw() else { return Err(wry::Error::UnsupportedWindowHandle) };
        let mtm = MainThreadMarker::new().expect("the page is built on the main thread");
        // SAFETY: an AppKit handle points at a live NSView, used here on the main thread.
        let host = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
        let frame = Self(NSView::initWithFrame(NSView::alloc(mtm), NSRect::ZERO));
        frame.0.setHidden(true);
        // As wry pins a child page: GPUI's view isn't flipped, so without it the frame drifts from the top as the window's height changes.
        frame.0.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinYMargin);
        host.addSubview(&frame.0);
        frame.place(rect);
        Ok(frame)
    }

    /// Call once the page is built inside, so it follows the frame's size.
    pub(super) fn fill(&self, view: &WebView) {
        let page = view.webview();
        page.setFrame(self.0.bounds());
        page.setAutoresizingMask(NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable);
    }

    pub(super) fn place(&self, rect: Rect) {
        // SAFETY: called on the main thread.
        let Some(host) = (unsafe { self.0.superview() }) else { return };
        let (x, y, width, height) = (rect.x.into(), rect.y.into(), rect.width.into(), rect.height.into());
        let y = if host.isFlipped() { y } else { host.frame().size.height - y - height };
        self.0.setFrame(NSRect::new(NSPoint::new(x, y), NSSize::new(width, height)));
    }

    pub(super) fn set_visible(&self, visible: bool) {
        self.0.setHidden(!visible);
    }
}

impl HasWindowHandle for Frame {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        let handle = AppKitWindowHandle::new(NonNull::from(&*self.0).cast());
        // SAFETY: the handle points at a view this frame retains.
        Ok(unsafe { WindowHandle::borrow_raw(handle.into()) })
    }
}

impl Drop for Frame {
    fn drop(&mut self) {
        self.0.removeFromSuperview();
    }
}

/// Built over the frame, so a press or the keys in the docked inspector count as the page's.
pub(super) struct Glue {
    cursor: cursor::Watch,
    _press: Option<press::Monitor>,
    pub(super) frame: Frame,
}

impl Glue {
    pub(super) fn new(frame: Frame, pressed: impl Fn() + 'static) -> Self {
        Self { cursor: cursor::Watch::new(&frame.0), _press: press::Monitor::new(&frame.0, pressed), frame }
    }

    pub(super) fn parked(&self) {
        self.cursor.release();
    }

    pub(super) fn holds_keys(&self) -> bool {
        holds_keys(&self.frame.0)
    }

    pub(super) fn give_keys(&self) {
        release(&self.frame.0);
    }
}

fn holds_keys(page: &NSView) -> bool {
    let Some(window) = page.window() else { return false };
    window.firstResponder().and_then(|r| r.downcast::<NSView>().ok()).is_some_and(|r| r.isDescendantOf(page))
}

/// GPUI's view declines first responder, so a press on it alone leaves the keys with the page.
fn release(page: &NSView) {
    // SAFETY: called on the main thread.
    if let (Some(window), Some(host)) = (page.window(), unsafe { page.superview() }) {
        window.makeFirstResponder(Some(&host));
    }
}

/// Sent down the key window's responder chain, which the page heads while it holds the keys.
pub(super) fn edit(edit: Edit) {
    let Some(mtm) = MainThreadMarker::new() else { return };
    let action = match edit {
        Edit::Copy => sel!(copy:),
        Edit::Cut => sel!(cut:),
        Edit::Paste => sel!(paste:),
        Edit::SelectAll => sel!(selectAll:),
        Edit::Undo => sel!(undo:),
        Edit::Redo => sel!(redo:),
    };
    // SAFETY: a nil target resolves along the responder chain, and each action takes a sender, which may be nil.
    unsafe { NSApplication::sharedApplication(mtm).sendAction_to_from(action, None, None) };
}

pub(super) fn closed(view: &WebView) -> bool {
    view.webview().window().is_none()
}

/// WKWebView names no browser, and some sites (Google) serve an unknown one a basic page.
pub(super) fn user_agent() -> &'static str {
    static AGENT: OnceLock<String> = OnceLock::new();
    AGENT.get_or_init(|| {
        let version = NSBundle::bundleWithPath(&NSString::from_str("/Applications/Safari.app"))
            .and_then(|safari| safari.objectForInfoDictionaryKey(&NSString::from_str("CFBundleShortVersionString")))
            .and_then(|version| version.downcast::<NSString>().ok())
            .map_or_else(|| "26.0".to_string(), |version| version.to_string());
        format!("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/{version} Safari/605.1.15")
    })
}
