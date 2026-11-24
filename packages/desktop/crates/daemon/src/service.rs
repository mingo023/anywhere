//! pocketd as a Login Item: the release app registers the LaunchAgent plist it ships.
//! On Linux and Windows the pocketd beside the app installs itself as a service instead (`bundled`).

#[cfg(not(target_os = "macos"))]
mod bundled;

#[cfg(not(target_os = "macos"))]
pub use bundled::{bundle_version, open_at_login, open_login_items, register, restart, status};
#[cfg(windows)]
pub use bundled::own_token;

/// What macOS reports for the bundled agent (`SMAppServiceStatus`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Service {
    NotRegistered,
    Enabled,
    /// The user switched it off in Login Items.
    RequiresApproval,
    NotFound,
}

impl Service {
    #[cfg(any(test, target_os = "macos"))]
    fn from_raw(raw: isize) -> Self {
        match raw {
            1 => Self::Enabled,
            2 => Self::RequiresApproval,
            3 => Self::NotFound,
            _ => Self::NotRegistered,
        }
    }
}

/// An agent that never registered reports NotFound as often as NotRegistered.
pub fn should_register(s: Service) -> bool {
    matches!(s, Service::NotRegistered | Service::NotFound)
}

#[cfg(target_os = "macos")]
fn plist() -> String {
    format!("{}.plist", channel::daemon_label())
}

#[cfg(target_os = "macos")]
fn agent() -> objc2::rc::Retained<objc2_service_management::SMAppService> {
    let name = objc2_foundation::NSString::from_str(&plist());
    unsafe { objc2_service_management::SMAppService::agentServiceWithPlistName(&name) }
}

/// Blocks on launchd; keep it off the UI thread.
#[cfg(target_os = "macos")]
pub fn status() -> Service {
    Service::from_raw(unsafe { agent().status() }.0)
}

/// Blocks on launchd; keep it off the UI thread.
#[cfg(target_os = "macos")]
pub fn register() -> Result<(), String> {
    unsafe { agent().registerAndReturnError() }.map_err(|e| e.localizedDescription().to_string())
}

pub fn register_if_needed() {
    if should_register(status())
        && let Err(e) = register()
    {
        eprintln!("pocketd service: {e}");
    }
}

/// Restarts the bundled service through launchd; pocketd resumes its agents on the way back.
#[cfg(target_os = "macos")]
pub fn restart() -> Result<(), String> {
    let uid = unsafe { libc::getuid() };
    let target = format!("gui/{uid}/{}", channel::daemon_label());
    let out = std::process::Command::new("launchctl").args(["kickstart", "-k", &target]).output().map_err(|e| e.to_string())?;
    if out.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&out.stderr).trim().to_string()) }
}

/// Whether opening at login must register (`Some(true)`) or unregister the app, given what macOS reports. One the user switched off in Login Items stays off.
pub fn login_step(want: bool, s: Service) -> Option<bool> {
    if want {
        should_register(s).then_some(true)
    } else {
        (s == Service::Enabled).then_some(false)
    }
}

/// Makes the app itself open at login, or not. Blocks on launchd; keep it off the UI thread.
#[cfg(target_os = "macos")]
pub fn open_at_login(on: bool) -> Result<(), String> {
    let app = unsafe { objc2_service_management::SMAppService::mainAppService() };
    let result = match login_step(on, Service::from_raw(unsafe { app.status() }.0)) {
        Some(true) => unsafe { app.registerAndReturnError() },
        Some(false) => unsafe { app.unregisterAndReturnError() },
        None => Ok(()),
    };
    result.map_err(|e| e.localizedDescription().to_string())
}

#[cfg(target_os = "macos")]
pub fn open_login_items() {
    unsafe { objc2_service_management::SMAppService::openSystemSettingsLoginItems() }
}

/// The version of the app bundle on disk, read fresh: an update may have swapped another build in under the running app.
/// Runs `plutil`; keep it off the UI thread.
#[cfg(target_os = "macos")]
pub fn bundle_version() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    plist_version(&exe.parent()?.parent()?.join("Info.plist"))
}

#[cfg(target_os = "macos")]
fn plist_version(plist: &std::path::Path) -> Option<String> {
    let out = crate::command("plutil").args(["-extract", "CFBundleShortVersionString", "raw", "-o", "-"]).arg(plist).output().ok()?;
    let version = String::from_utf8(out.stdout).ok()?.trim().to_string();
    (out.status.success() && !version.is_empty()).then_some(version)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "macos")]
    #[test]
    fn the_bundle_version_is_read_from_the_info_plist_on_disk() {
        let dir = std::env::temp_dir().join(format!("bundle-version-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let plist = dir.join("Info.plist");
        std::fs::write(&plist, r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict><key>CFBundleShortVersionString</key><string>0.1.61</string></dict></plist>"#).unwrap();
        let read = plist_version(&plist);
        let missing = plist_version(&dir.join("gone.plist"));
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!((read.as_deref(), missing), (Some("0.1.61"), None));
    }

    #[test]
    fn status_codes_map_to_service_states() {
        let got: Vec<Service> = [0, 1, 2, 3, 7].into_iter().map(Service::from_raw).collect();
        assert_eq!(got, [Service::NotRegistered, Service::Enabled, Service::RequiresApproval, Service::NotFound, Service::NotRegistered]);
    }

    #[test]
    fn a_never_registered_agent_gets_registered() {
        assert!(should_register(Service::NotRegistered));
        assert!(should_register(Service::NotFound));
    }

    #[test]
    fn opening_at_login_changes_only_what_macos_does_not_already_have() {
        assert_eq!(login_step(true, Service::NotRegistered), Some(true));
        assert_eq!(login_step(true, Service::RequiresApproval), None);
        assert_eq!(login_step(true, Service::Enabled), None);
        assert_eq!(login_step(false, Service::Enabled), Some(false));
        assert_eq!(login_step(false, Service::NotFound), None);
    }

    #[test]
    fn an_agent_running_or_switched_off_by_the_user_is_left_alone() {
        assert!(!should_register(Service::Enabled));
        assert!(!should_register(Service::RequiresApproval));
    }
}
