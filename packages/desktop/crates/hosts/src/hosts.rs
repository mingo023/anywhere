use agents::hosts::{Entry, Failure};
use std::path::{Path, PathBuf};

pub use agents::hosts::Target;

pub mod checklist;
pub mod owner;
pub mod remote;
pub mod session;
pub mod ssh_config;

/// A machine running Anywhere that this Mac's pocketd links over SSH.
#[derive(Clone, Debug, PartialEq)]
pub struct Host {
    /// Names its files on this Mac.
    pub id: String,
    pub name: String,
    pub target: Target,
    /// Its pocketd is reached on its owner TCP port with a token, not on its socket.
    pub windows: bool,
    /// Where this Mac's end of the tunnel to the host's pocketd listens, as pocketd says.
    pub socket: PathBuf,
}

impl From<&Entry> for Host {
    fn from(e: &Entry) -> Self {
        Self { id: e.id.clone(), name: e.name.clone(), target: e.target.clone(), windows: e.windows, socket: e.socket.clone().into() }
    }
}

impl Host {
    /// The owner token a Windows host's pocketd gave this Mac, which it keeps beside the socket.
    pub fn owner_file(&self) -> PathBuf {
        self.socket.with_extension("token")
    }

    /// The tunnel's socket, with the owner token when there is one. Reads a file.
    pub fn endpoint(&self) -> daemon::Endpoint {
        daemon::Endpoint { path: self.socket.clone(), token: owner::load(&self.owner_file()).map(|o| o.token) }
    }

    /// `user@addr:port` as typed, or the alias with the user and port given over its config's.
    pub fn address(&self) -> String {
        let (name, port, user) = match &self.target {
            Target::Alias { name, port, user } => (name, *port, user.as_deref().unwrap_or_default()),
            Target::Address { addr, port, user, .. } => (addr, Some(*port).filter(|p| *p != 22), user.as_str()),
        };
        let user = if user.is_empty() { String::new() } else { format!("{user}@") };
        let port = port.map(|p| format!(":{p}")).unwrap_or_default();
        format!("{user}{name}{port}")
    }
}

/// What went wrong on `name`, and what to do about it.
pub fn failure_message(failure: Failure, name: &str) -> String {
    match failure {
        Failure::Auth => format!("{name} didn't accept your SSH keys. Check you can run ssh {name} in Terminal."),
        Failure::UnknownKey => format!("{name}'s SSH key isn't trusted yet. Connect once with ssh {name} in Terminal."),
        Failure::KeyChanged => format!("{name}'s SSH key changed. Check with whoever runs it before continuing."),
        Failure::NoSuchHost => format!("Can't find {name}. Check the address, or your connection."),
        Failure::Refused => format!("{name} refused the connection. Check that SSH is on there."),
        Failure::TimedOut => format!("Can't reach {name}. Check it's on and on your network."),
        Failure::Dropped => format!("Lost the connection to {name}."),
        Failure::Throttled => format!("{name} is limiting sign-ins after failed attempts. Trying again shortly."),
        Failure::NotRunning => format!("Anywhere is stopped on {name}."),
        Failure::Unpaired => format!("Anywhere on {name} didn't accept this Mac. Reconnect to pair again."),
        Failure::PathTooLong => format!("The path to Anywhere on {name} is too long to forward."),
        Failure::Other => format!("Couldn't connect to {name}."),
    }
}

/// The folder of pocketd archives this Mac can upload: `ANYWHERE_HOST_ARCHIVE_DIR`, or in a dev build the one
/// scripts/package-pocketd.sh fills. pocketd, a LaunchAgent, has neither, so the desktop names it in an install.
pub fn dist_dir() -> Option<PathBuf> {
    std::env::var_os("ANYWHERE_HOST_ARCHIVE_DIR")
        .map(PathBuf::from)
        .or_else(|| (!channel::is_release()).then(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../pocketd/dist")))
}

/// What the add form typed, as a target: a bare name `~/.ssh/config` has is that alias.
pub fn target(addr: &str, port: &str, user: &str, key_file: Option<&str>, aliases: &[String]) -> Result<Target, &'static str> {
    let (addr, user) = (checked_address(addr)?, checked_user(user)?);
    let port = checked_port(port)?.unwrap_or(22);
    if port == 22 && user.is_empty() && key_file.is_none() && aliases.iter().any(|a| a == addr) {
        return Ok(Target::alias(addr));
    }
    Ok(Target::Address { addr: addr.into(), port, user: user.into(), key_file: key_file.map(Into::into) })
}

/// What the form typed for an alias of `~/.ssh/config`: a port or user typed wins over the config's.
pub fn alias(name: &str, port: &str, user: &str) -> Result<Target, &'static str> {
    let (name, user) = (checked_address(name)?, checked_user(user)?);
    Ok(Target::Alias { name: name.into(), port: checked_port(port)?, user: (!user.is_empty()).then(|| user.into()) })
}

/// `old` as the edit form typed it: an alias stays one, and an address keeps its key file.
pub fn edited(old: &Target, addr: &str, port: &str, user: &str) -> Result<Target, &'static str> {
    match old {
        Target::Alias { .. } => alias(addr, port, user),
        Target::Address { key_file, .. } => target(addr, port, user, key_file.as_deref(), &[]),
    }
}

/// The edit form's address, port and user for `target`; an alias's blank ones are its config's.
pub fn fields(target: &Target) -> [String; 3] {
    match target {
        Target::Alias { name, port, user } => [name.clone(), port.map(|p| p.to_string()).unwrap_or_default(), user.clone().unwrap_or_default()],
        Target::Address { addr, port, user, .. } => [addr.clone(), port.to_string(), user.clone()],
    }
}

fn checked_address(addr: &str) -> Result<&str, &'static str> {
    let addr = addr.trim();
    if addr.is_empty() {
        return Err("Enter the host's address or name");
    }
    if addr.contains(char::is_whitespace) || addr.starts_with('-') {
        return Err("Enter an address like 10.0.4.12 or devbox.example.com");
    }
    Ok(addr)
}

/// A blank port is `None`.
fn checked_port(port: &str) -> Result<Option<u16>, &'static str> {
    match port.trim() {
        "" => Ok(None),
        p => p.parse::<u16>().ok().filter(|p| *p > 0).map(Some).ok_or("The port is a number, usually 22"),
    }
}

fn checked_user(user: &str) -> Result<&str, &'static str> {
    let user = user.trim();
    if user.contains(char::is_whitespace) || user.starts_with('-') {
        return Err("Enter a username without spaces");
    }
    Ok(user)
}

/// The desktop's key for `path` on host `id`: `//<id>/<path>`, with a Windows path's `\` as `/`.
pub fn place(id: &str, path: &str) -> String {
    let path = path.replace('\\', "/");
    if path.starts_with('/') { format!("//{id}{path}") } else { format!("//{id}/{path}") }
}

/// The host id and the path on it that `key` names; `None` and the key itself for a path on this Mac.
pub fn split(key: &str) -> (Option<&str>, &str) {
    let Some((id, path)) = key.strip_prefix("//").and_then(|rest| rest.find('/').map(|i| rest.split_at(i))) else { return (None, key) };
    let drive = path.get(1..).filter(|p| on_windows(p));
    (Some(id), drive.unwrap_or(path))
}

/// Whether a host's `path` starts with a drive, as only Windows paths do.
pub fn on_windows(path: &str) -> bool {
    matches!(path.as_bytes(), [d, b':', ..] if d.is_ascii_alphabetic())
}

/// Where git for the tree at `key` runs: this Mac, its host while linked, else a runner that only fails.
pub fn runner_for(key: &str) -> git::Runner {
    match split(key) {
        (None, _) => git::Runner::Local,
        (Some(id), _) => match remote::connected(id) {
            Some(client) => git::Runner::Remote(client),
            None => git::Runner::Remote(std::sync::Arc::new(git::NotConnected)),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn address(addr: &str) -> Target {
        Target::Address { addr: addr.into(), port: 22, user: String::new(), key_file: None }
    }

    #[test]
    fn the_address_leaves_out_the_default_port_and_an_empty_user() {
        let host = |target| Host { id: "h".into(), name: "h".into(), target, windows: false, socket: PathBuf::new() };
        assert_eq!(host(Target::Address { addr: "10.0.4.12".into(), port: 2222, user: "dev".into(), key_file: None }).address(), "dev@10.0.4.12:2222");
        assert_eq!(host(address("devbox")).address(), "devbox");
    }

    #[test]
    fn an_alias_shows_the_user_and_port_given_over_its_config() {
        let host = |target| Host { id: "h".into(), name: "h".into(), target, windows: false, socket: PathBuf::new() };
        assert_eq!(host(Target::alias("devbox")).address(), "devbox");
        assert_eq!(host(Target::Alias { name: "devbox".into(), port: Some(22), user: Some("ops".into()) }).address(), "ops@devbox:22");
    }

    #[test]
    fn an_alias_typed_with_a_port_or_user_keeps_them_and_blank_ones_are_its_configs() {
        assert_eq!(alias(" pi-lab ", "", ""), Ok(Target::alias("pi-lab")));
        assert_eq!(alias("pi-lab", "2201", "pi"), Ok(Target::Alias { name: "pi-lab".into(), port: Some(2201), user: Some("pi".into()) }));
        assert_eq!(alias("pi-lab", "0", ""), Err("The port is a number, usually 22"));
        assert_eq!(alias("pi-lab", "65536", ""), Err("The port is a number, usually 22"));
        assert_eq!(alias("pi-lab", "", "a b"), Err("Enter a username without spaces"));
        assert_eq!(alias("", "", ""), Err("Enter the host's address or name"));
    }

    #[test]
    fn an_edit_keeps_the_kind_of_target_and_an_addresss_key_file() {
        let keyed = Target::Address { addr: "10.0.4.12".into(), port: 2222, user: "dev".into(), key_file: Some("~/.ssh/k".into()) };
        assert_eq!(fields(&keyed), ["10.0.4.12".to_string(), "2222".into(), "dev".into()]);
        assert_eq!(
            edited(&keyed, "10.0.4.13", "22", "ops"),
            Ok(Target::Address { addr: "10.0.4.13".into(), port: 22, user: "ops".into(), key_file: Some("~/.ssh/k".into()) })
        );
        assert!(matches!(edited(&address("x"), "devbox", "", ""), Ok(Target::Address { .. })));
        let devbox = Target::alias("devbox");
        assert_eq!(fields(&devbox), ["devbox".to_string(), String::new(), String::new()]);
        assert_eq!(edited(&devbox, "devbox", "2222", "ops"), Ok(Target::Alias { name: "devbox".into(), port: Some(2222), user: Some("ops".into()) }));
        assert_eq!(edited(&devbox, "devbox", "", ""), Ok(devbox.clone()));
        assert_eq!(edited(&devbox, "", "", ""), Err("Enter the host's address or name"));
    }

    #[test]
    fn a_name_from_ssh_config_is_kept_as_its_alias() {
        let aliases = ["devbox".to_string()];
        assert_eq!(target(" devbox ", "", "", None, &aliases), Ok(Target::alias("devbox")));
        assert_eq!(target("devbox", "", "root", None, &aliases), Ok(Target::Address { addr: "devbox".into(), port: 22, user: "root".into(), key_file: None }));
    }

    #[test]
    fn the_add_form_says_what_is_wrong_with_what_was_typed() {
        assert_eq!(target("", "", "", None, &[]), Err("Enter the host's address or name"));
        assert_eq!(target("10.0.4.12", "ssh", "", None, &[]), Err("The port is a number, usually 22"));
        assert_eq!(target("-oProxyCommand=x", "", "", None, &[]), Err("Enter an address like 10.0.4.12 or devbox.example.com"));
        assert_eq!(target("10.0.4.12", "", "-x", None, &[]), Err("Enter a username without spaces"));
        assert_eq!(
            target("10.0.4.12", "2222", "", Some("~/.ssh/k"), &[]),
            Ok(Target::Address { addr: "10.0.4.12".into(), port: 2222, user: String::new(), key_file: Some("~/.ssh/k".into()) })
        );
    }

    #[test]
    fn a_host_entry_from_the_wire_keeps_its_target() {
        let target = Target::Address { addr: "10.0.4.12".into(), port: 2222, user: "dev".into(), key_file: Some("~/.ssh/id_ed25519".into()) };
        let entry = Entry {
            id: "10-0-4-12".into(),
            name: "build box".into(),
            target: target.clone(),
            windows: true,
            socket: "/h/10-0-4-12.sock".into(),
            link: agents::hosts::Link::Connecting,
        };
        assert_eq!(Host::from(&entry), Host { id: "10-0-4-12".into(), name: "build box".into(), target, windows: true, socket: "/h/10-0-4-12.sock".into() });
    }

    #[test]
    fn a_hosts_endpoint_is_the_socket_pocketd_reports_and_the_token_it_keeps_beside_it() {
        let dir = std::env::temp_dir().join(format!("host-endpoint-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("win.token"), r#"{"token":"t0k","port":7}"#).unwrap();
        let entry = |id: &str| Entry {
            id: id.into(),
            name: id.into(),
            target: address("10.0.4.12"),
            windows: true,
            socket: dir.join(format!("{id}.sock")).to_string_lossy().into_owned(),
            link: agents::hosts::Link::Connecting,
        };
        let with_token = Host::from(&entry("win")).endpoint();
        assert_eq!((with_token.path, with_token.token), (dir.join("win.sock"), Some("t0k".to_string())));
        let without = Host::from(&entry("mac")).endpoint();
        assert_eq!((without.path, without.token), (dir.join("mac.sock"), None));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_host_path_is_keyed_under_the_host_id() {
        assert_eq!(place("devbox", "/home/me/app"), "//devbox/home/me/app");
        assert_eq!(split("//devbox/home/me/app"), (Some("devbox"), "/home/me/app"));
    }

    #[test]
    fn a_windows_path_is_keyed_with_forward_slashes_and_comes_back_without_a_leading_one() {
        assert_eq!(place("win", "C:\\Users\\me\\app"), "//win/C:/Users/me/app");
        assert_eq!(split("//win/C:/Users/me/app"), (Some("win"), "C:/Users/me/app"));
    }

    #[test]
    fn a_path_on_this_mac_names_no_host() {
        assert_eq!(split("/Users/me/app"), (None, "/Users/me/app"));
        assert_eq!(split("local-1"), (None, "local-1"));
    }

    #[test]
    fn git_for_a_host_key_never_runs_on_this_mac() {
        assert!(matches!(runner_for("/Users/me/app"), git::Runner::Local));
        let remote = runner_for("//devbox/Users/me/app");
        assert!(matches!(remote, git::Runner::Remote(_)));
        assert_eq!(remote.stat("/").unwrap_err().kind(), std::io::ErrorKind::NotConnected);
    }
}
