use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{MainThreadMarker, Message};
use objc2_app_kit::{NSEvent, NSEventMask, NSView};
use std::ptr::NonNull;

/// Watches presses in the page's window before AppKit dispatches them, so focus follows a press between the page and GPUI's view.
pub(super) struct Monitor(Retained<AnyObject>);

impl Monitor {
    pub(super) fn new(page: &NSView, pressed: impl Fn() + 'static) -> Option<Self> {
        let mtm = MainThreadMarker::new()?;
        let page = page.retain();
        let block = RcBlock::new(move |event: NonNull<NSEvent>| -> *mut NSEvent {
            // SAFETY: AppKit hands a valid event.
            match landed(&page, unsafe { event.as_ref() }, mtm) {
                Some(Landed::Page) => pressed(),
                Some(Landed::Host) if super::holds_keys(&page) => super::release(&page),
                _ => {}
            }
            event.as_ptr()
        });
        let mask = NSEventMask::LeftMouseDown | NSEventMask::RightMouseDown | NSEventMask::OtherMouseDown;
        // SAFETY: the block returns the event it was handed.
        unsafe { NSEvent::addLocalMonitorForEventsMatchingMask_handler(mask, &block) }.map(Self)
    }
}

impl Drop for Monitor {
    fn drop(&mut self) {
        // SAFETY: a monitor `addLocalMonitorForEventsMatchingMask:handler:` returned, removed once.
        unsafe { NSEvent::removeMonitor(&self.0) };
    }
}

enum Landed {
    Page,
    /// GPUI's view, outside the page.
    Host,
}

/// A hidden page takes no hit.
fn landed(page: &NSView, event: &NSEvent, mtm: MainThreadMarker) -> Option<Landed> {
    let (window, pressed) = (page.window()?, event.window(mtm)?);
    if !std::ptr::eq(&*window, &*pressed) {
        return None;
    }
    // SAFETY: called on the main thread.
    let host = unsafe { page.superview() }?;
    // SAFETY: called on the main thread.
    let frame = unsafe { host.superview() }?;
    // `hitTest:` takes a point in the superview's coordinates.
    let hit = host.hitTest(frame.convertPoint_fromView(event.locationInWindow(), None))?;
    if hit.isDescendantOf(page) {
        Some(Landed::Page)
    } else if std::ptr::eq(&*hit, &*host) {
        Some(Landed::Host)
    } else {
        None
    }
}
