//! Sparkle, loaded at run time from the app's Frameworks so dev builds, tests and captures never need it.
use super::Event;
use block2::{Block, RcBlock};
use futures::channel::mpsc::UnboundedSender;
use objc2::rc::{Allocated, Retained};
use objc2::runtime::{AnyClass, AnyObject, Bool, NSObject, NSObjectProtocol};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_foundation::{NSBundle, NSString};
use std::cell::RefCell;

pub struct Ivars {
    tx: UnboundedSender<Event>,
    install: RefCell<Option<RcBlock<dyn Fn()>>>,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements, and Delegate doesn't implement Drop.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "PocketUpdaterDelegate"]
    #[ivars = Ivars]
    struct Delegate;

    unsafe impl NSObjectProtocol for Delegate {}

    impl Delegate {
        /// The update is downloaded and verified. Keeping the block lets "Restart to update" install it now; Sparkle installs it at quit anyway.
        #[unsafe(method(updater:willInstallUpdateOnQuit:immediateInstallationBlock:))]
        fn will_install_on_quit(&self, _updater: &AnyObject, item: &AnyObject, install: &Block<dyn Fn()>) -> Bool {
            // SAFETY: SUAppcastItem.displayVersionString is a non-null NSString.
            let version: Retained<NSString> = unsafe { msg_send![item, displayVersionString] };
            *self.ivars().install.borrow_mut() = Some(install.copy());
            let _ = self.ivars().tx.unbounded_send(Event::Ready(version.to_string()));
            Bool::YES
        }
    }
);

/// The running SPUStandardUpdaterController. Sparkle holds its delegate weakly, so this keeps it alive.
pub struct Sparkle {
    controller: Retained<AnyObject>,
    delegate: Retained<Delegate>,
}

impl Sparkle {
    /// `None` when the app carries no Sparkle.framework, or off the main thread.
    pub fn start(tx: UnboundedSender<Event>) -> Option<Self> {
        let mtm = MainThreadMarker::new()?;
        let frameworks = NSBundle::mainBundle().privateFrameworksPath()?;
        let bundle = NSBundle::bundleWithPath(&NSString::from_str(&format!("{frameworks}/Sparkle.framework")))?;
        // SAFETY: the framework is the one release-mac.sh bundled and signed; loading it runs only its own initialisers.
        if !unsafe { bundle.load() } {
            return None;
        }
        let class = AnyClass::get(c"SPUStandardUpdaterController")?;
        let delegate = Delegate::alloc(mtm).set_ivars(Ivars { tx, install: RefCell::default() });
        // SAFETY: NSObject's init on a freshly allocated subclass.
        let delegate: Retained<Delegate> = unsafe { msg_send![super(delegate), init] };
        // SAFETY: matches -initWithStartingUpdater:updaterDelegate:userDriverDelegate: (SPUStandardUpdaterController.h:97).
        let controller: Allocated<AnyObject> = unsafe { msg_send![class, alloc] };
        let controller: Retained<AnyObject> = unsafe {
            msg_send![controller, initWithStartingUpdater: Bool::YES, updaterDelegate: &*delegate, userDriverDelegate: None::<&AnyObject>]
        };
        Some(Self { controller, delegate })
    }

    pub fn check(&self) {
        // SAFETY: -checkForUpdates: takes a nullable sender.
        let _: () = unsafe { msg_send![&*self.controller, checkForUpdates: None::<&AnyObject>] };
    }

    /// Installs the ready update and relaunches; `None` until one is ready.
    pub fn installer(&self) -> Option<Box<dyn FnOnce()>> {
        let install = self.delegate.ivars().install.borrow().clone()?;
        Some(Box::new(move || install.call(())))
    }
}
