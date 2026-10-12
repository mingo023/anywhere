//! The ssh commands that reach a host, and what their failures mean.
//! The options and stderr patterns come from docs/orchestrators/research/21-ssh-connection-spike.md.

use crate::{Host, Target};
use base64::{Engine, engine::general_purpose::STANDARD};
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
pub fn args(host: &Host, known_hosts: &Path, extra: &[String], command: &str) -> Vec<String> {
    let mut out: Vec<String> = [
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
    ]
    .iter()
    .flat_map(|o| ["-o".to_string(), o.to_string()])
    .collect();
    // The app's file first, so a newly accepted key lands there and `~/.ssh/known_hosts` stays as it is.
    out.extend(["-o".into(), format!("UserKnownHostsFile=\"{}\" ~/.ssh/known_hosts", known_hosts.display())]);
    out.extend(extra.iter().cloned());
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

/// Runs `script` in Windows PowerShell whatever the account's ssh shell is: cmd.exe by default, whose quoting
/// it never meets, encoded as `-EncodedCommand` wants it.
pub fn powershell(script: &str) -> String {
    let utf16: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    format!("powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand {}", STANDARD.encode(utf16))
}

/// `text` as a PowerShell literal.
pub fn ps_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

/// Prints the host's home, then whether the app's pocketd is installed under it, from PowerShell.
pub fn windows_probe_command(home_dir_name: &str) -> String {
    let shim = ps_quote(&format!(r"{home_dir_name}\bin\pocketd.cmd"));
    powershell(&format!("Write-Output $HOME; if (Test-Path (Join-Path $HOME {shim})) {{ 'installed' }} else {{ 'missing' }}"))
}

/// The Windows probe's answer; a home that isn't `X:\…` means this wasn't Windows.
pub fn parse_windows_probe(stdout: &str) -> Option<Probe> {
    let mut lines = stdout.lines().map(str::trim).filter(|l| !l.is_empty());
    let (status, home) = (lines.next_back()?, lines.next_back()?);
    let drive = matches!(home.as_bytes(), [d, b':', b'\\', ..] if d.is_ascii_alphabetic());
    drive.then(|| Probe { home: home.into(), running: status == "installed" })
}

/// `TUNNEL_COMMAND` for Windows, where nothing like `cat` holds stdin.
pub fn windows_tunnel_command() -> String {
    powershell("[Console]::Out.WriteLine('READY'); [Console]::Out.Flush(); $null = [Console]::In.ReadToEnd()")
}

/// The options that forward `local` to pocketd's owner TCP port on the host: Windows' sshd can't forward to a socket.
pub fn tcp_forward(local: &Path, port: u16) -> Result<Vec<String>, Failure> {
    let local = local.display().to_string();
    if local.len() > MAX_SOCKET_PATH {
        return Err(Failure::PathTooLong);
    }
    Ok(["-o", "ExitOnForwardFailure=yes", "-o", "StreamLocalBindUnlink=yes", "-L"]
        .iter()
        .map(|s| s.to_string())
        .chain([format!("{local}:127.0.0.1:{port}")])
        .collect())
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
    /// A Windows host's pocketd refused this Mac's owner token, even a newly paired one.
    Unpaired,
    PathTooLong,
    Other,
}

impl Failure {
    /// How long to wait before trying again after `attempt` failures in a row; `None` waits for the user.
    pub fn retry_after(self, attempt: u32) -> Option<Duration> {
        let backoff = |cap: u64| Duration::from_secs((1u64 << attempt.min(6)).min(cap));
        match self {
            // Retrying a refused login only extends sshd's penalty, and the rest won't change by waiting.
            Failure::Auth | Failure::UnknownKey | Failure::KeyChanged | Failure::Unpaired | Failure::PathTooLong => None,
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
            Failure::Unpaired => format!("Anywhere on {name} didn't accept this Mac. Reconnect to pair again."),
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
        let args = args(&address(22, "", None), Path::new("/h/known_hosts"), &[], "true");
        let opts: Vec<&str> = args.iter().map(String::as_str).collect();
        assert!(opts.contains(&"BatchMode=yes") && opts.contains(&"StrictHostKeyChecking=accept-new") && opts.contains(&"ControlMaster=no"));
        assert!(opts.contains(&"UserKnownHostsFile=\"/h/known_hosts\" ~/.ssh/known_hosts"));
        assert_eq!(&opts[opts.len() - 2..], ["10.0.4.12", "true"]);
    }

    #[test]
    fn an_address_passes_only_the_port_user_and_key_it_was_given() {
        let tail = |h: &Host| args(h, Path::new("/k"), &[], "true").into_iter().skip_while(|a| !a.starts_with("UserKnownHostsFile")).skip(1).collect::<Vec<_>>();
        assert_eq!(tail(&address(22, "", None)), ["--", "10.0.4.12", "true"]);
        assert_eq!(tail(&address(2222, "dev", Some("/k/id"))), ["-p", "2222", "-l", "dev", "-i", "/k/id", "-o", "IdentitiesOnly=yes", "--", "10.0.4.12", "true"]);
        let alias = Host::new(Target::Alias { name: "-oProxyCommand=x".into() }, &[]);
        assert_eq!(tail(&alias), ["--", "-oProxyCommand=x", "true"], "a saved name never becomes an option");
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

    fn decoded(command: &str) -> String {
        let b64 = command.strip_prefix("powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand ").unwrap();
        let bytes = STANDARD.decode(b64).unwrap();
        String::from_utf16(&bytes.chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect::<Vec<_>>()).unwrap()
    }

    #[test]
    fn powershell_gets_the_script_as_utf16_so_cmds_quoting_never_touches_it() {
        let command = windows_probe_command(".it's");
        assert!(command.bytes().all(|b| b.is_ascii_alphanumeric() || b" -+/=".contains(&b)), "{command}");
        assert_eq!(decoded(&command), r"Write-Output $HOME; if (Test-Path (Join-Path $HOME '.it''s\bin\pocketd.cmd')) { 'installed' } else { 'missing' }");
        assert!(decoded(&windows_tunnel_command()).contains("ReadToEnd()"));
    }

    #[test]
    fn the_windows_probe_wants_a_drive_home_so_a_unix_one_is_not_mistaken_for_it() {
        assert_eq!(parse_windows_probe("Welcome\r\nC:\\Users\\me\r\ninstalled\r\n"), Some(Probe { home: r"C:\Users\me".into(), running: true }));
        assert_eq!(parse_windows_probe("C:\\Users\\me\nmissing\n"), Some(Probe { home: r"C:\Users\me".into(), running: false }));
        assert_eq!(parse_windows_probe("/home/dev\nmissing\n"), None);
        assert_eq!(parse_windows_probe("'powershell' is not recognized\n"), None);
    }

    #[test]
    fn a_windows_forward_goes_to_the_owner_port_on_the_hosts_loopback() {
        let opts = tcp_forward(Path::new("/l/pc.sock"), 4519).unwrap();
        assert_eq!(opts, ["-o", "ExitOnForwardFailure=yes", "-o", "StreamLocalBindUnlink=yes", "-L", "/l/pc.sock:127.0.0.1:4519"]);
        assert_eq!(tcp_forward(&PathBuf::from(format!("/{}", "x".repeat(110))), 4519), Err(Failure::PathTooLong));
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
