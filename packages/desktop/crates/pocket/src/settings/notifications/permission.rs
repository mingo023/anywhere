use futures::channel::oneshot;

/// Whether macOS refuses the app's banners. `None` outside an app bundle, where asking aborts the process.
#[cfg(target_os = "macos")]
pub(crate) fn denied() -> Option<oneshot::Receiver<bool>> {
    use block2::RcBlock;
    use objc2_user_notifications::{UNAuthorizationStatus, UNNotificationSettings, UNUserNotificationCenter};
    use std::{ptr::NonNull, sync::Mutex};
    bundle_id()?;
    let (tx, rx) = oneshot::channel();
    let tx = Mutex::new(Some(tx));
    let done = RcBlock::new(move |settings: NonNull<UNNotificationSettings>| {
        let denied = unsafe { settings.as_ref() }.authorizationStatus() == UNAuthorizationStatus::Denied;
        if let Some(tx) = tx.lock().ok().and_then(|mut t| t.take()) {
            let _ = tx.send(denied);
        }
    });
    UNUserNotificationCenter::currentNotificationCenter().getNotificationSettingsWithCompletionHandler(&done);
    Some(rx)
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn denied() -> Option<oneshot::Receiver<bool>> {
    None
}

#[cfg(target_os = "macos")]
pub(crate) fn bundle_id() -> Option<String> {
    objc2_foundation::NSBundle::mainBundle().bundleIdentifier().map(|id| id.to_string())
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn bundle_id() -> Option<String> {
    None
}

/// The app's page in System Settings › Notifications.
pub(crate) fn settings_url(bundle_id: &str) -> String {
    format!("x-apple.systempreferences:com.apple.Notifications-Settings.extension?id={bundle_id}")
}

#[cfg(test)]
mod tests {
    use super::settings_url;

    #[test]
    fn the_settings_link_opens_this_app_in_notifications() {
        assert_eq!(settings_url("dev.anywhere.app"), "x-apple.systempreferences:com.apple.Notifications-Settings.extension?id=dev.anywhere.app");
    }
}
