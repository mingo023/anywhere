use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub mod install;
pub mod link;
pub mod ssh;
pub mod ssh_config;

/// How ssh reaches a host.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Target {
    /// A `Host` in `~/.ssh/config`; ssh brings its user, port, key and jump host.
    Alias { name: String },
    Address {
        addr: String,
        port: u16,
        /// Empty signs in as the user on this Mac.
        user: String,
        key_file: Option<String>,
    },
}

/// A machine running Anywhere that this Mac reaches over SSH.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Host {
    /// Names its files on this Mac; short so its socket path fits a unix socket's limit.
    pub id: String,
    pub name: String,
    pub target: Target,
}

impl Host {
    /// A host for `target` whose id none of `taken` has.
    pub fn new(target: Target, taken: &[Host]) -> Self {
        let name = match &target {
            Target::Alias { name } => name.clone(),
            Target::Address { addr, .. } => short_name(addr),
        };
        let id = unique(&slug(&name), taken);
        Self { id, name, target }
    }

    /// Where this Mac's end of the tunnel to the host's pocketd listens.
    pub fn socket(&self, home: &Path) -> PathBuf {
        home.join("hosts").join(format!("{}.sock", self.id))
    }

    /// The host keys this app accepted for the host; removing the host forgets them.
    pub fn known_hosts(&self, home: &Path) -> PathBuf {
        home.join("hosts").join(format!("{}.known_hosts", self.id))
    }

    /// The key this app signs in with after a password sign-in put it on the host; its public half is `<key>.pub`.
    pub fn key(&self, home: &Path) -> PathBuf {
        home.join("hosts").join(format!("{}.key", self.id))
    }

    /// `user@addr:port` as typed, or the alias.
    pub fn address(&self) -> String {
        match &self.target {
            Target::Alias { name } => name.clone(),
            Target::Address { addr, port, user, .. } => {
                let user = if user.is_empty() { String::new() } else { format!("{user}@") };
                let port = if *port == 22 { String::new() } else { format!(":{port}") };
                format!("{user}{addr}{port}")
            }
        }
    }
}

/// What the add form typed, as a target: a bare name `~/.ssh/config` has is that alias.
pub fn target(addr: &str, port: &str, user: &str, key_file: Option<&str>, aliases: &[String]) -> Result<Target, &'static str> {
    let (addr, port, user) = (addr.trim(), port.trim(), user.trim());
    if addr.is_empty() {
        return Err("Enter the host's address or name");
    }
    if addr.contains(char::is_whitespace) || addr.starts_with('-') {
        return Err("Enter an address like 10.0.4.12 or devbox.example.com");
    }
    let port = match port {
        "" => 22,
        p => p.parse::<u16>().ok().filter(|p| *p > 0).ok_or("The port is a number, usually 22")?,
    };
    if user.contains(char::is_whitespace) || user.starts_with('-') {
        return Err("Enter a username without spaces");
    }
    if port == 22 && user.is_empty() && key_file.is_none() && aliases.iter().any(|a| a == addr) {
        return Ok(Target::Alias { name: addr.into() });
    }
    Ok(Target::Address { addr: addr.into(), port, user: user.into(), key_file: key_file.map(Into::into) })
}

/// `devbox` for `devbox.example.com`; an IP address stays whole.
fn short_name(addr: &str) -> String {
    let first = addr.split('.').next().unwrap_or(addr);
    if addr.parse::<std::net::IpAddr>().is_ok() || first.is_empty() { addr.to_string() } else { first.to_string() }
}

const ID_LEN: usize = 24;

fn slug(name: &str) -> String {
    let s: String = name.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' }).take(ID_LEN).collect();
    let s = s.trim_matches('-');
    if s.is_empty() { "host".into() } else { s.into() }
}

fn unique(base: &str, taken: &[Host]) -> String {
    let free = |id: &str| !taken.iter().any(|h| h.id == id);
    if free(base) {
        return base.into();
    }
    (2..).map(|n| format!("{base}-{n}")).find(|id| free(id)).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn address(addr: &str) -> Target {
        Target::Address { addr: addr.into(), port: 22, user: String::new(), key_file: None }
    }

    #[test]
    fn a_host_is_named_for_its_alias_or_the_first_part_of_its_address() {
        assert_eq!(Host::new(Target::Alias { name: "devbox".into() }, &[]).name, "devbox");
        assert_eq!(Host::new(address("gpu-1.lab.example.com"), &[]).name, "gpu-1");
        assert_eq!(Host::new(address("10.0.4.12"), &[]).name, "10.0.4.12");
    }

    #[test]
    fn a_second_host_with_the_same_name_gets_its_own_id() {
        let first = Host::new(address("devbox.a.com"), &[]);
        let second = Host::new(address("devbox.b.com"), std::slice::from_ref(&first));
        assert_eq!((first.id.as_str(), second.id.as_str()), ("devbox", "devbox-2"));
    }

    #[test]
    fn a_long_name_still_fits_a_unix_socket_path() {
        let long = "a-really-long-host-alias-somebody-typed-into-their-ssh-config.internal.example.com";
        let host = Host::new(Target::Alias { name: long.into() }, &[]);
        let home = Path::new("/Users/someone-with-a-long-username/.coding-pocket-dev");
        assert!(host.socket(home).as_os_str().len() < 104, "{}", host.socket(home).display());
    }

    #[test]
    fn the_address_leaves_out_the_default_port_and_an_empty_user() {
        let host = Host::new(Target::Address { addr: "10.0.4.12".into(), port: 2222, user: "mingo".into(), key_file: None }, &[]);
        assert_eq!(host.address(), "mingo@10.0.4.12:2222");
        assert_eq!(Host::new(address("devbox"), &[]).address(), "devbox");
    }

    #[test]
    fn a_name_from_ssh_config_is_kept_as_its_alias() {
        let aliases = ["devbox".to_string()];
        assert_eq!(target(" devbox ", "", "", None, &aliases), Ok(Target::Alias { name: "devbox".into() }));
        assert_eq!(target("devbox", "", "root", None, &aliases), Ok(Target::Address { addr: "devbox".into(), port: 22, user: "root".into(), key_file: None }));
    }

    #[test]
    fn the_add_form_says_what_is_wrong_with_what_was_typed() {
        assert_eq!(target("", "", "", None, &[]), Err("Enter the host's address or name"));
        assert_eq!(target("10.0.4.12", "ssh", "", None, &[]), Err("The port is a number, usually 22"));
        assert_eq!(target("-oProxyCommand=x", "", "", None, &[]), Err("Enter an address like 10.0.4.12 or devbox.example.com"));
        assert_eq!(target("10.0.4.12", "", "-x", None, &[]), Err("Enter a username without spaces"));
        assert_eq!(target("10.0.4.12", "2222", "", Some("~/.ssh/k"), &[]), Ok(Target::Address { addr: "10.0.4.12".into(), port: 2222, user: String::new(), key_file: Some("~/.ssh/k".into()) }));
    }

    #[test]
    fn hosts_round_trip_through_json() {
        let hosts = vec![
            Host::new(Target::Alias { name: "devbox".into() }, &[]),
            Host::new(Target::Address { addr: "10.0.4.12".into(), port: 22, user: "u".into(), key_file: Some("~/.ssh/k".into()) }, &[]),
        ];
        let raw = serde_json::to_string(&hosts).unwrap();
        assert_eq!(serde_json::from_str::<Vec<Host>>(&raw).unwrap(), hosts);
    }
}
