use crate::actions::CheckForUpdates;
use crate::desktop::Desktop;
use futures::StreamExt;
use futures::channel::mpsc;
use gpui_kit::*;
use store::Store;

#[cfg(target_os = "macos")]
mod sparkle;

/// A version's release page, for "What's new".
pub fn release_url(version: &str) -> String {
    format!("https://github.com/mingo023/anywhere/releases/tag/v{version}")
}

/// What the sidebar says about updates.
#[derive(Clone, Debug, PartialEq)]
pub enum Update {
    Idle,
    /// Downloaded and verified; installs at quit, or now on "Restart to update".
    Ready { version: String },
    /// The app relaunched on `version`.
    WhatsNew { version: String },
}

impl Update {
    pub fn ready(&mut self, version: String) {
        *self = Update::Ready { version };
    }

    pub fn dismiss(&mut self) {
        if matches!(self, Update::WhatsNew { .. }) {
            *self = Update::Idle;
        }
    }
}

/// What a launch on `version` has to say, recording it as seen. A first install has nothing new.
pub fn launch(version: &str, seen: &mut Option<String>) -> Update {
    match seen.replace(version.to_string()) {
        Some(was) if was != version => Update::WhatsNew { version: version.into() },
        _ => Update::Idle,
    }
}

/// Sent from Sparkle's delegate on the main thread.
pub enum Event {
    Ready(String),
}

/// The sidebar's update state, and Sparkle when this build carries it.
pub struct Updates {
    pub update: Update,
    sparkle: Option<sparkle::Sparkle>,
}

impl Updates {
    /// Starts Sparkle in a release build; dev builds have no feed.
    pub fn new(store: &mut Store, cx: &mut Context<Desktop>) -> Self {
        let mut updates = Self { update: Update::Idle, sparkle: None };
        if !channel::is_release() {
            return updates;
        }
        let before = store.seen_version.clone();
        updates.update = launch(channel::version(), &mut store.seen_version);
        if store.seen_version != before
            && let Some((path, raw)) = store.encode()
        {
            cx.background_executor().spawn(async move { store::write(&path, &raw) }).detach();
        }
        let (tx, mut rx) = mpsc::unbounded();
        updates.sparkle = sparkle::Sparkle::start(tx);
        cx.spawn(async move |this, cx| {
            while let Some(event) = rx.next().await {
                if this.update(cx, |d, cx| d.on_update(event, cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
        updates
    }

    pub fn available(&self) -> bool {
        self.sparkle.is_some()
    }

    /// Sparkle's own "Check for Updates…" dialogs.
    pub fn check(&self) {
        if let Some(s) = &self.sparkle {
            s.check();
        }
    }

    fn installer(&self) -> Option<Box<dyn FnOnce()>> {
        self.sparkle.as_ref().and_then(|s| s.installer())
    }
}

/// No Sparkle off macOS.
#[cfg(not(target_os = "macos"))]
mod sparkle {
    pub struct Sparkle;

    impl Sparkle {
        pub fn start(_: futures::channel::mpsc::UnboundedSender<super::Event>) -> Option<Self> {
            None
        }

        pub fn check(&self) {}

        pub fn installer(&self) -> Option<Box<dyn FnOnce()>> {
            None
        }
    }
}

impl Desktop {
    fn on_update(&mut self, event: Event, cx: &mut Context<Self>) {
        match event {
            Event::Ready(version) => self.updates.update.ready(version),
        }
        cx.notify();
    }

    pub(crate) fn check_for_updates(&mut self, _: &CheckForUpdates, _: &mut Window, _: &mut Context<Self>) {
        self.updates.check();
    }

    pub(crate) fn restart_to_update(&mut self, cx: &mut Context<Self>) {
        let Some(install) = self.updates.installer() else { return };
        // Sparkle terminates the app inside the call, and AppKit's terminate re-enters GPUI, so it can't run inside this update.
        cx.spawn(async move |_, _| install()).detach();
    }

    pub(crate) fn dismiss_whats_new(&mut self, cx: &mut Context<Self>) {
        self.updates.update.dismiss();
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::{Update, launch};

    fn whats_new(v: &str) -> Update {
        Update::WhatsNew { version: v.into() }
    }

    #[test]
    fn a_first_launch_has_nothing_new() {
        let mut seen = None;
        assert_eq!(launch("0.1.0", &mut seen), Update::Idle);
        assert_eq!(seen.as_deref(), Some("0.1.0"));
    }

    #[test]
    fn a_relaunch_on_a_new_version_says_whats_new_once() {
        let mut seen = Some("0.1.0".to_string());
        assert_eq!(launch("0.2.0", &mut seen), whats_new("0.2.0"));
        assert_eq!(launch("0.2.0", &mut seen), Update::Idle);
    }

    #[test]
    fn a_ready_update_replaces_whats_new() {
        let mut u = whats_new("0.2.0");
        u.ready("0.3.0".into());
        assert_eq!(u, Update::Ready { version: "0.3.0".into() });
    }

    #[test]
    fn dismissing_clears_whats_new_but_not_a_ready_update() {
        let mut u = whats_new("0.2.0");
        u.dismiss();
        assert_eq!(u, Update::Idle);
        let mut u = Update::Ready { version: "0.3.0".into() };
        u.dismiss();
        assert_eq!(u, Update::Ready { version: "0.3.0".into() });
    }
}
