//! The owner token a Windows host's pocketd mints for this Mac. Its sockets can't tell who the owner is,
//! so every connection to its owner TCP port sends the token first.

use crate::Host;
use crate::link::Paths;
use crate::ssh::{self, Failure};
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::Path;
use std::process::Stdio;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Owner {
    pub token: String,
    /// pocketd's owner TCP port on the host's loopback.
    pub port: u16,
}

pub fn load(path: &Path) -> Option<Owner> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

/// Writes `owner` readable only by this user.
pub fn save(path: &Path, owner: &Owner) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        crate::private_dir(dir)?;
    }
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options.open(path)?;
    // `mode` only applies to a file this creates.
    #[cfg(unix)]
    file.set_permissions(std::os::unix::fs::PermissionsExt::from_mode(0o600))?;
    file.write_all(&serde_json::to_vec(owner)?)
}

/// What `pocketd pair --json` printed, past any login noise.
pub fn parse_pair(stdout: &str) -> Option<Owner> {
    stdout.lines().rev().find_map(|l| serde_json::from_str(l.trim()).ok())
}

/// Mints a token named `name` with the app's pocketd under the host's home.
pub fn pair_command(home_dir_name: &str, name: &str) -> String {
    // cmd.exe runs the shim and reads its arguments in the console's code page.
    let name: String = name.chars().map(|c| if c == '\u{2019}' { '\'' } else if c.is_ascii() && c != '"' { c } else { '?' }).collect();
    let shim = ssh::ps_quote(&format!(r"{home_dir_name}\bin\pocketd.cmd"));
    ssh::powershell(&format!("& (Join-Path $HOME {shim}) pair --json --name {}; exit $LASTEXITCODE", ssh::ps_quote(&name)))
}

/// Pairs this Mac with the host's pocketd and keeps the token beside the host's other files here.
pub fn pair(host: &Host, paths: &Paths) -> Result<Owner, Failure> {
    let command = pair_command(&paths.remote_dir, &mac_name());
    let out = crate::ssh_command().args(ssh::args(host, &host.known_hosts(&paths.home), host.signed_in_key(&paths.home).as_deref(), &[], &command)).stdin(Stdio::null()).output().map_err(|_| Failure::Other)?;
    if out.status.code() == Some(255) {
        return Err(ssh::classify(&String::from_utf8_lossy(&out.stderr)));
    }
    let owner = parse_pair(&String::from_utf8_lossy(&out.stdout)).ok_or(Failure::Other)?;
    save(&host.owner_file(&paths.home), &owner).map_err(|_| Failure::Other)?;
    Ok(owner)
}

/// This Mac's name as Sharing shows it, so `pocketd owner list` on the host says whose token it is.
fn mac_name() -> String {
    #[cfg(target_os = "macos")]
    let name = std::process::Command::new("scutil").args(["--get", "ComputerName"]).output().ok().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
    #[cfg(windows)]
    let name = std::env::var("COMPUTERNAME").ok();
    #[cfg(all(unix, not(target_os = "macos")))]
    let name = std::fs::read_to_string("/etc/hostname").ok().map(|n| n.trim().to_string());
    name.filter(|n| !n.is_empty()).unwrap_or_else(|| if cfg!(target_os = "macos") { "Mac" } else { "Desktop" }.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(unix)]
    fn the_token_file_is_only_the_users_even_when_it_was_there_before() {
        use std::fs::Permissions;
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("hosts-owner-{}", std::process::id()));
        let path = dir.join("hosts/pc.token");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "old").unwrap();
        std::fs::set_permissions(&path, Permissions::from_mode(0o644)).unwrap();
        let owner = Owner { token: "t1".into(), port: 4519 };
        save(&path, &owner).unwrap();
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(load(&path), Some(owner));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn pair_reads_the_token_and_port_past_any_login_noise() {
        let out = "Welcome to the box\r\n{\"token\":\"abc\",\"id\":\"45bd\",\"port\":4519}\r\n";
        assert_eq!(parse_pair(out), Some(Owner { token: "abc".into(), port: 4519 }));
        assert_eq!(parse_pair("ownerListen is off; set it in config.json\n"), None);
    }
}
