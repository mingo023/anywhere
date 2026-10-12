//! The ssh commands that reach a host, and what their failures mean.
//! The options and stderr patterns come from docs/orchestrators/research/21-ssh-connection-spike.md.

use crate::{Host, Target};
use std::path::Path;
use std::time::Duration;

/// Both ends of a forwarded unix socket must fit `sun_path` on macOS, which ssh checks before connecting.
const MAX_SOCKET_PATH: usize = 103;

/// Prints the host's `$HOME`, then whether pocketd's socket is there; the forward needs an absolute path.
pub fn probe_command(home_dir_name: &str) -> String {
    format!(r#"printf '%s\n' "$HOME"; test -S "$HOME/{home_dir_name}/pocketd.sock" && echo running || echo stopped"#)
}

/// Holds the session open after the forward is up, so `READY` on stdout means the socket is ready.
pub const TUNNEL_COMMAND: &str = "echo READY; exec cat";

/// What the probe found: the host's home folder and whether pocketd runs there.
#[derive(Debug, PartialEq)]
pub struct Probe {
    pub home: String,
    pub running: bool,
}

pub fn parse_probe(stdout: &str) -> Option<Probe> {
    let mut lines = stdout.lines().map(str::trim).filter(|l| !l.is_empty());
    let (status, home) = (lines.next_back()?, lines.next_back()?);
    home.starts_with('/').then(|| Probe { home: home.into(), running: status == "running" })
}

/// ssh's arguments for `command` on `host`, never prompting, with `extra` options before the destination.
/// App options come first because ssh keeps the first value it reads for an option.
/// `key` is the app's own key for the host; with it, keys ssh wasn't named aren't offered, so the user's agent can't spend `MaxAuthTries` first.
pub fn args(host: &Host, known_hosts: &Path, key: Option<&Path>, extra: &[String], command: &str) -> Vec<String> {
    let mut out = options(&[
        "BatchMode=yes",
        "ConnectTimeout=10",
        "ServerAliveInterval=5",
        "ServerAliveCountMax=3",
        "StrictHostKeyChecking=accept-new",
        // Overrides a QUIET or ERROR in the user's config, which would hide the lines `classify` reads.
        "LogLevel=INFO",
        // A user's `ControlMaster auto` would hand the forward to a background master and exit at once.
        "ControlMaster=no",
        "ControlPath=none",
    ]);
    out.extend(known_hosts_option(known_hosts));
    if let Some(key) = key {
        out.extend(["-i".into(), key.display().to_string(), "-o".into(), "IdentitiesOnly=yes".into()]);
    }
    out.extend(extra.iter().cloned());
    out.extend(destination(host, command));
    out
}

/// ssh's arguments for `command` on `host`, signing in by `method` with the password askpass gives it.
/// Its own list rather than `args`', whose `BatchMode=yes` would win and refuse to ask.
pub fn password_args(host: &Host, known_hosts: &Path, method: PasswordMethod, command: &str) -> Vec<String> {
    let mut out = options(&[
        "BatchMode=no",
        "PubkeyAuthentication=no",
        method.option(),
        // Each wrong password adds to sshd's penalty, which also locks out good keys.
        "NumberOfPasswordPrompts=1",
        // The password goes only to the host key the key sign-in recorded, never to a new one.
        "StrictHostKeyChecking=yes",
        "ConnectTimeout=10",
        "LogLevel=INFO",
        "ControlMaster=no",
        "ControlPath=none",
    ]);
    out.extend(known_hosts_option(known_hosts));
    out.extend(destination(host, command));
    out
}

/// One method per connection: after a wrong password, OpenSSH 9.6 on Ubuntu never answers a
/// keyboard-interactive attempt in the same connection, so ssh hangs until the server's login grace ends.
#[derive(Clone, Copy)]
pub enum PasswordMethod {
    Password,
    KeyboardInteractive,
}

impl PasswordMethod {
    fn option(self) -> &'static str {
        match self {
            PasswordMethod::Password => "PreferredAuthentications=password",
            PasswordMethod::KeyboardInteractive => "PreferredAuthentications=keyboard-interactive",
        }
    }
}

/// Whether a refused ssh's stderr says the host takes plain passwords, so keyboard-interactive is only worth a try when it doesn't.
pub fn takes_passwords(stderr: &str) -> bool {
    let Some((_, rest)) = stderr.split_once("Permission denied (") else { return true };
    rest.split(')').next().unwrap_or_default().split(',').any(|m| m == "password")
}

/// Adds the public key on stdin to the host's `authorized_keys` unless it's there, on a line of its own
/// even when the file doesn't end in a newline. Wrapped in `sh -c` because the host's login shell may not be POSIX.
pub const INSTALL_KEY: &str = r#"sh -c 'umask 077; mkdir -p ~/.ssh; chmod 700 ~/.ssh; touch ~/.ssh/authorized_keys; chmod 600 ~/.ssh/authorized_keys; key=$(cat); grep -qxF "$key" ~/.ssh/authorized_keys && exit; [ -z "$(tail -c1 ~/.ssh/authorized_keys)" ] || echo >> ~/.ssh/authorized_keys; printf "%s\n" "$key" >> ~/.ssh/authorized_keys'"#;

fn options(opts: &[&str]) -> Vec<String> {
    opts.iter().flat_map(|o| ["-o".to_string(), o.to_string()]).collect()
}

/// The app's file first, so a newly accepted key lands there and `~/.ssh/known_hosts` stays as it is.
fn known_hosts_option(known_hosts: &Path) -> [String; 2] {
    ["-o".into(), format!("UserKnownHostsFile=\"{}\" ~/.ssh/known_hosts", known_hosts.display())]
}

fn destination(host: &Host, command: &str) -> Vec<String> {
    let mut out = Vec::new();
    let destination = match &host.target {
        Target::Alias { name } => name.clone(),
        Target::Address { addr, port, user, key_file } => {
            if *port != 22 {
                out.extend(["-p".into(), port.to_string()]);
            }
            if !user.is_empty() {
                out.extend(["-l".into(), user.clone()]);
            }
            if let Some(key) = key_file {
                out.extend(["-i".into(), key.clone(), "-o".into(), "IdentitiesOnly=yes".into()]);
            }
            addr.clone()
        }
    };
    // A saved host is read back from disk, so one starting with `-` must not become an option.
    out.extend(["--".into(), destination, command.into()]);
    out
}

/// The options that forward `local` to pocketd's socket under the host's `home`.
pub fn forward(local: &Path, home: &str, home_dir_name: &str) -> Result<Vec<String>, Failure> {
    let remote = format!("{}/{home_dir_name}/pocketd.sock", home.trim_end_matches('/'));
    let local = local.display().to_string();
    if local.len() > MAX_SOCKET_PATH || remote.len() > MAX_SOCKET_PATH {
        return Err(Failure::PathTooLong);
    }
    Ok([
        "-o",
        "ExitOnForwardFailure=yes",
        // ssh never removes its socket file, so the next ssh can't bind without this.
        "-o",
        "StreamLocalBindUnlink=yes",
        "-L",
    ]
    .iter()
    .map(|s| s.to_string())
    .chain([format!("{local}:{remote}")])
    .collect())
}

/// Why a connection failed, by what the user can do about it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Failure {
    /// Wrong key, no key, a key with a passphrase and no agent, or a wrong user: ssh can't tell them apart.
    Auth,
    UnknownKey,
    KeyChanged,
    NoSuchHost,
    Refused,
    TimedOut,
    /// The link went quiet, or the host closed it.
    Dropped,
    /// sshd's penalty for failed logins, which also blocks good ones for about 20 seconds.
    Throttled,
    NotRunning,
    PathTooLong,
    Other,
}

impl Failure {
    /// How long to wait before trying again after `attempt` failures in a row; `None` waits for the user.
    pub fn retry_after(self, attempt: u32) -> Option<Duration> {
        let backoff = |cap: u64| Duration::from_secs((1u64 << attempt.min(6)).min(cap));
        match self {
            // Retrying a refused login only extends sshd's penalty, and the rest won't change by waiting.
            Failure::Auth | Failure::UnknownKey | Failure::KeyChanged | Failure::PathTooLong => None,
            Failure::Throttled => Some(Duration::from_secs(20)),
            // pocketd comes back after the host's sshd when the host restarts.
            Failure::NotRunning => Some(backoff(15).max(Duration::from_secs(2))),
            // What ssh said isn't known, so it may never work.
            Failure::Other => (attempt < 3).then(|| backoff(15)),
            // A name that doesn't resolve may only mean this Mac is offline.
            Failure::NoSuchHost | Failure::Refused | Failure::TimedOut | Failure::Dropped => Some(backoff(15)),
        }
    }

    /// What went wrong on `name`, and what to do about it.
    pub fn message(self, name: &str) -> String {
        match self {
            Failure::Auth => format!("{name} didn't accept your SSH keys. Check you can run ssh {name} in Terminal."),
            Failure::UnknownKey => format!("{name}'s SSH key isn't trusted yet. Connect once with ssh {name} in Terminal."),
            Failure::KeyChanged => format!("{name}'s SSH key changed. Check with whoever runs it before continuing."),
            Failure::NoSuchHost => format!("Can't find {name}. Check the address, or your connection."),
            Failure::Refused => format!("{name} refused the connection. Check that SSH is on there."),
            Failure::TimedOut => format!("Can't reach {name}. Check it's on and on your network."),
            Failure::Dropped => format!("Lost the connection to {name}."),
            Failure::Throttled => format!("{name} is limiting sign-ins after failed attempts. Trying again shortly."),
            Failure::NotRunning => format!("Anywhere is stopped on {name}."),
            Failure::PathTooLong => format!("The path to Anywhere on {name} is too long to forward."),
            Failure::Other => format!("Couldn't connect to {name}."),
        }
    }
}

/// What a failed ssh's stderr says went wrong, checked most specific first.
pub fn classify(stderr: &str) -> Failure {
    const PATTERNS: &[(&str, Failure)] = &[
        ("REMOTE HOST IDENTIFICATION HAS CHANGED", Failure::KeyChanged),
        ("forwarding disabled due to host key", Failure::KeyChanged),
        ("has changed and you have requested strict checking", Failure::KeyChanged),
        ("Host key verification failed", Failure::UnknownKey),
        ("Permission denied (", Failure::Auth),
        ("Bad local forwarding specification", Failure::PathTooLong),
        ("too long for Unix domain socket", Failure::PathTooLong),
        ("Could not resolve hostname", Failure::NoSuchHost),
        ("Connection refused", Failure::Refused),
        ("timed out", Failure::TimedOut),
        ("not responding", Failure::Dropped),
        ("closed by remote host", Failure::Dropped),
        ("Connection closed by", Failure::Throttled),
    ];
    PATTERNS.iter().find(|(text, _)| stderr.contains(text)).map_or(Failure::Other, |(_, f)| *f)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn address(port: u16, user: &str, key_file: Option<&str>) -> Host {
        Host::new(Target::Address { addr: "10.0.4.12".into(), port, user: user.into(), key_file: key_file.map(Into::into) }, &[])
    }

    #[test]
    fn ssh_never_prompts_and_keeps_new_host_keys_out_of_the_users_file() {
        let args = args(&address(22, "", None), Path::new("/h/known_hosts"), None, &[], "true");
        let opts: Vec<&str> = args.iter().map(String::as_str).collect();
        assert!(opts.contains(&"BatchMode=yes") && opts.contains(&"StrictHostKeyChecking=accept-new") && opts.contains(&"ControlMaster=no"));
        assert!(opts.contains(&"UserKnownHostsFile=\"/h/known_hosts\" ~/.ssh/known_hosts"));
        assert_eq!(&opts[opts.len() - 2..], ["10.0.4.12", "true"]);
    }

    #[test]
    fn an_address_passes_only_the_port_user_and_key_it_was_given() {
        let tail = |h: &Host| args(h, Path::new("/k"), None, &[], "true").into_iter().skip_while(|a| !a.starts_with("UserKnownHostsFile")).skip(1).collect::<Vec<_>>();
        assert_eq!(tail(&address(22, "", None)), ["--", "10.0.4.12", "true"]);
        assert_eq!(tail(&address(2222, "dev", Some("/k/id"))), ["-p", "2222", "-l", "dev", "-i", "/k/id", "-o", "IdentitiesOnly=yes", "--", "10.0.4.12", "true"]);
        let alias = Host::new(Target::Alias { name: "-oProxyCommand=x".into() }, &[]);
        assert_eq!(tail(&alias), ["--", "-oProxyCommand=x", "true"], "a saved name never becomes an option");
    }

    #[test]
    fn the_apps_own_key_goes_first_and_the_users_agent_keys_not_at_all() {
        let args = args(&address(22, "", Some("/k/id")), Path::new("/k"), Some(Path::new("/h/devbox.key")), &[], "true");
        let at = |key: &str| args.iter().position(|a| a == key).unwrap();
        assert!(at("/h/devbox.key") < at("/k/id"));
        let no_key_file = super::args(&address(22, "", None), Path::new("/k"), Some(Path::new("/h/devbox.key")), &[], "true");
        assert!(no_key_file.iter().any(|a| a == "IdentitiesOnly=yes"));
    }

    #[test]
    fn installing_a_key_adds_it_once_on_its_own_line() {
        let home = std::env::temp_dir().join(format!("pocket-install-key-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join(".ssh")).unwrap();
        std::fs::write(home.join(".ssh/authorized_keys"), "ssh-ed25519 AAAAold me").unwrap();
        let install = || {
            let mut sh = std::process::Command::new("sh").args(["-c", INSTALL_KEY]).env("HOME", &home).stdin(std::process::Stdio::piped()).spawn().unwrap();
            std::io::Write::write_all(&mut sh.stdin.take().unwrap(), b"ssh-ed25519 AAAAnew anywhere\n").unwrap();
            assert!(sh.wait().unwrap().success());
        };
        install();
        install();
        assert_eq!(std::fs::read_to_string(home.join(".ssh/authorized_keys")).unwrap(), "ssh-ed25519 AAAAold me\nssh-ed25519 AAAAnew anywhere\n");
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn a_password_sign_in_asks_once_and_only_a_host_already_trusted() {
        let args = password_args(&address(2222, "dev", Some("/k/id")), Path::new("/h/known_hosts"), PasswordMethod::Password, INSTALL_KEY);
        let opts: Vec<&str> = args.iter().map(String::as_str).collect();
        assert!(opts.contains(&"BatchMode=no") && opts.contains(&"PubkeyAuthentication=no") && opts.contains(&"NumberOfPasswordPrompts=1"));
        assert!(opts.contains(&"PreferredAuthentications=password"), "one method, or a wrong password hangs on the next");
        assert!(opts.contains(&"StrictHostKeyChecking=yes") && !opts.contains(&"StrictHostKeyChecking=accept-new") && !opts.contains(&"BatchMode=yes"));
        assert!(opts.contains(&"UserKnownHostsFile=\"/h/known_hosts\" ~/.ssh/known_hosts"), "a host trusted from Terminal counts");
        assert_eq!(&opts[opts.len() - 3..], ["--", "10.0.4.12", INSTALL_KEY]);
    }

    #[test]
    fn keyboard_interactive_is_tried_only_on_a_host_without_plain_passwords() {
        assert!(takes_passwords("dev@10.0.4.12: Permission denied (publickey,password,keyboard-interactive)."));
        assert!(!takes_passwords("dev@10.0.4.12: Permission denied (publickey,keyboard-interactive)."));
        assert!(takes_passwords("ssh: connect to host 10.0.4.12 port 22: Connection refused"), "a failure before sign-in isn't about the method");
    }

    #[test]
    fn the_forward_goes_to_pocketds_socket_under_the_hosts_absolute_home() {
        let opts = forward(Path::new("/l/devbox.sock"), "/home/dev/", ".coding-pocket").unwrap();
        assert_eq!(opts.last().unwrap(), "/l/devbox.sock:/home/dev/.coding-pocket/pocketd.sock");
        assert!(opts.contains(&"StreamLocalBindUnlink=yes".to_string()));
    }

    #[test]
    fn a_home_too_long_for_a_socket_path_fails_before_ssh_runs() {
        let home = format!("/home/{}", "x".repeat(90));
        assert_eq!(forward(&PathBuf::from("/l/a.sock"), &home, ".coding-pocket"), Err(Failure::PathTooLong));
    }

    #[test]
    fn the_probe_reads_home_and_whether_pocketd_runs_past_any_login_noise() {
        assert_eq!(parse_probe("Welcome!\n/home/dev\nrunning\n"), Some(Probe { home: "/home/dev".into(), running: true }));
        assert_eq!(parse_probe("/Users/m\nstopped\n"), Some(Probe { home: "/Users/m".into(), running: false }));
        assert_eq!(parse_probe("stopped\n"), None);
    }

    #[test]
    fn stderr_tells_what_went_wrong() {
        let cases = [
            ("dev@10.0.4.12: Permission denied (publickey,password,keyboard-interactive).", Failure::Auth),
            ("No ED25519 host key is known for [127.0.0.1]:2230 and you have requested strict checking.\nHost key verification failed.", Failure::UnknownKey),
            ("@ WARNING: REMOTE HOST IDENTIFICATION HAS CHANGED! @\nHost key verification failed.", Failure::KeyChanged),
            ("Host key for [127.0.0.1]:2250 has changed and you have requested strict checking.\nHost key verification failed.", Failure::KeyChanged),
            ("ssh: Could not resolve hostname nosuch.invalid: nodename nor servname provided, or not known", Failure::NoSuchHost),
            ("ssh: connect to host 127.0.0.1 port 2230: Connection refused", Failure::Refused),
            ("ssh: connect to host 10.255.255.1 port 22: Operation timed out", Failure::TimedOut),
            ("Connection timed out during banner exchange", Failure::TimedOut),
            ("Timeout, server 127.0.0.1 not responding.", Failure::Dropped),
            ("Connection to 127.0.0.1 closed by remote host.", Failure::Dropped),
            ("Connection closed by 127.0.0.1 port 2230", Failure::Throttled),
            ("Bad local forwarding specification '/l:/r'", Failure::PathTooLong),
            ("", Failure::Other),
        ];
        for (stderr, failure) in cases {
            assert_eq!(classify(stderr), failure, "{stderr}");
        }
    }

    #[test]
    fn a_refused_login_waits_for_the_user_and_a_lost_link_backs_off() {
        assert_eq!(Failure::Auth.retry_after(0), None);
        assert_eq!(Failure::KeyChanged.retry_after(3), None);
        assert_eq!(Failure::Throttled.retry_after(0), Some(Duration::from_secs(20)));
        let waits: Vec<u64> = (0..7).map(|n| Failure::Dropped.retry_after(n).unwrap().as_secs()).collect();
        assert_eq!(waits, [1, 2, 4, 8, 15, 15, 15]);
        assert_eq!(Failure::NotRunning.retry_after(0), Some(Duration::from_secs(2)));
    }

    #[test]
    fn an_unknown_ssh_failure_gets_three_tries() {
        let tries: Vec<bool> = (0..4).map(|n| Failure::Other.retry_after(n).is_some()).collect();
        assert_eq!(tries, [true, true, true, false]);
    }
}
