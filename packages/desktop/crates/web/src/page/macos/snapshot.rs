use block2::RcBlock;
use objc2::MainThreadMarker;
use objc2_app_kit::NSImage;
use objc2_foundation::NSError;
use objc2_web_kit::WKSnapshotConfiguration;
use std::cell::Cell;
use wry::{WebView, WebViewExtMacOS};

pub(crate) fn snapshot(view: &WebView, done: impl FnOnce(Option<Vec<u8>>) + 'static) {
    let Some(mtm) = MainThreadMarker::new() else { return done(None) };
    // SAFETY: on the main thread, as `mtm` proves.
    let config = unsafe { WKSnapshotConfiguration::new(mtm) };
    // The page is parked right after this; waiting for its next screen update may never call back for a hidden view.
    // SAFETY: on the main thread, as `mtm` proves.
    unsafe { config.setAfterScreenUpdates(false) };
    let done = Cell::new(Some(done));
    let block = RcBlock::new(move |image: *mut NSImage, _: *mut NSError| {
        // SAFETY: WebKit passes a live image, or null when the snapshot failed.
        let tiff = unsafe { image.as_ref() }.and_then(|image| image.TIFFRepresentation()).map(|data| data.to_vec());
        if let Some(done) = done.take() {
            done(tiff);
        }
    });
    // SAFETY: on the main thread, like every call into the page; WebKit retains the block until it calls it.
    unsafe { view.webview().takeSnapshotWithConfiguration_completionHandler(Some(&config), &block) };
}
