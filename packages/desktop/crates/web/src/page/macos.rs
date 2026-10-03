mod cursor;
mod press;

use super::Edit;
use objc2::{MainThreadMarker, sel};
use objc2_app_kit::{NSApplication, NSView};
use objc2_foundation::{NSBundle, NSString};
use std::sync::OnceLock;
use wry::{WebView, WebViewExtMacOS};

pub(super) struct Glue {
    cursor: cursor::Watch,
    _press: Option<press::Monitor>,
}

impl Glue {
    pub(super) fn new(view: &WebView, pressed: impl Fn() + 'static) -> Self {
        let page = view.webview();
        Self { cursor: cursor::Watch::new(&page), _press: press::Monitor::new(&page, pressed) }
    }

    pub(super) fn parked(&self) {
        self.cursor.release();
    }
}

pub(super) fn holds(view: &WebView) -> bool {
    holds_keys(&view.webview())
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
