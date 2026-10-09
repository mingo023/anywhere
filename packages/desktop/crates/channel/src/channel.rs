//! Release or Dev, fixed at build time: `scripts/release-mac.sh` sets `ANYWHERE_CHANNEL=release`
//! and every other build is Dev, so a dev build never touches the installed app (ADR 0004).
//! Mirrors `config.Release` and its siblings in pocketd.

struct Identity {
    home_dir_name: &'static str,
    bundle_id: &'static str,
    daemon_label: &'static str,
}

const RELEASE: Identity = Identity {
    home_dir_name: ".coding-pocket",
    bundle_id: "dev.mingo.anywhere",
    daemon_label: "dev.mingo.anywhere.pocketd",
};

const DEV: Identity = Identity {
    home_dir_name: ".coding-pocket-dev",
    bundle_id: "dev.mingo.anywhere.dev",
    daemon_label: "dev.mingo.anywhere.dev.pocketd",
};

pub fn is_release() -> bool {
    option_env!("ANYWHERE_CHANNEL") == Some("release")
}

/// The release version, without its `v`, or "dev".
pub fn version() -> &'static str {
    option_env!("ANYWHERE_VERSION").unwrap_or("dev")
}

pub fn home_dir_name() -> &'static str {
    identity(is_release()).home_dir_name
}

pub fn bundle_id() -> &'static str {
    identity(is_release()).bundle_id
}

pub fn daemon_label() -> &'static str {
    identity(is_release()).daemon_label
}

/// The phone port pocketd takes until one is set; mirrors `config.DefaultPort`.
pub fn phone_port() -> u32 {
    if is_release() { 4517 } else { 4518 }
}

fn identity(release: bool) -> &'static Identity {
    if release { &RELEASE } else { &DEV }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_and_dev_share_no_home_bundle_or_daemon() {
        let (release, dev) = (identity(true), identity(false));
        assert_ne!(release.home_dir_name, dev.home_dir_name);
        assert_ne!(release.bundle_id, dev.bundle_id);
        assert_ne!(release.daemon_label, dev.daemon_label);
    }

    #[test]
    fn release_keeps_the_names_installed_copies_depend_on() {
        let r = identity(true);
        assert_eq!(
            (r.home_dir_name, r.bundle_id, r.daemon_label),
            (".coding-pocket", "dev.mingo.anywhere", "dev.mingo.anywhere.pocketd")
        );
    }

    #[test]
    fn a_plain_build_is_dev() {
        assert!(!is_release());
        assert_eq!(home_dir_name(), ".coding-pocket-dev");
        assert_eq!(version(), "dev");
    }
}
