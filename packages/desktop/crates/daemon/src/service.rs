//! pocketd as a Login Item: the release app registers the LaunchAgent plist it ships.

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

#[cfg(not(target_os = "macos"))]
pub fn status() -> Service {
    Service::NotFound
}

/// Blocks on launchd; keep it off the UI thread.
#[cfg(target_os = "macos")]
pub fn register() -> Result<(), String> {
    unsafe { agent().registerAndReturnError() }.map_err(|e| e.localizedDescription().to_string())
}

#[cfg(not(target_os = "macos"))]
pub fn register() -> Result<(), String> {
    Err(format!("{} needs macOS", plist()))
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

#[cfg(not(target_os = "macos"))]
pub fn restart() -> Result<(), String> {
    Err(format!("{} needs macOS", plist()))
}

#[cfg(target_os = "macos")]
pub fn open_login_items() {
    unsafe { objc2_service_management::SMAppService::openSystemSettingsLoginItems() }
}

#[cfg(not(target_os = "macos"))]
pub fn open_login_items() {}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn an_agent_running_or_switched_off_by_the_user_is_left_alone() {
        assert!(!should_register(Service::Enabled));
        assert!(!should_register(Service::RequiresApproval));
    }
}
