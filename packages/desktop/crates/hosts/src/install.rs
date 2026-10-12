//! Signs in to a host once with a password, to give it a key of the app's own.

use crate::Host;
use crate::ssh::{self, Failure, PasswordMethod};
use std::io::{Read, Write};
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Askpass reads the password from here: an inherited fd doesn't reach it, and argv shows in `ps`.
const PASSWORD_VAR: &str = "POCKET_SSH_PW";

/// ssh's `ConnectTimeout` doesn't bound a server that stops answering mid sign-in.
const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(30);

/// Puts the app's key for `host` in its `authorized_keys`, signing in with `password`; blocks until ssh is done.
/// The host's key must already be trusted, so the password never goes to a host nobody checked.
pub fn with_password(host: &Host, home: &Path, password: &str) -> Result<(), Failure> {
    let dir = home.join("hosts");
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&dir).map_err(|_| Failure::Other)?;
    let key = host.app_key(home);
    if !key.exists() {
        keygen(&key)?;
    }
    let public = std::fs::read(public_key(&key)).map_err(|_| Failure::Other)?;
    let askpass = dir.join("askpass");
    std::fs::write(&askpass, format!("#!/bin/sh\nprintf '%s\\n' \"${PASSWORD_VAR}\"\n")).map_err(|_| Failure::Other)?;
    std::fs::set_permissions(&askpass, std::fs::Permissions::from_mode(0o700)).map_err(|_| Failure::Other)?;
    match sign_in(host, home, &askpass, password, &public, PasswordMethod::Password) {
        Err(stderr) if !ssh::takes_passwords(&stderr) => sign_in(host, home, &askpass, password, &public, PasswordMethod::KeyboardInteractive),
        signed_in => signed_in,
    }
    .map_err(|stderr| ssh::classify(&stderr))
}

/// Runs the key install over one password sign-in; ssh's stderr when it fails.
fn sign_in(host: &Host, home: &Path, askpass: &Path, password: &str, public: &[u8], method: PasswordMethod) -> Result<(), String> {
    let mut child = Command::new("ssh")
        .args(ssh::password_args(host, &host.known_hosts(home), method, ssh::INSTALL_KEY))
        .env("SSH_ASKPASS", askpass)
        // Askpass even when the app was started from a terminal, whose tty ssh would otherwise prompt on.
        .env("SSH_ASKPASS_REQUIRE", "force")
        .env(PASSWORD_VAR, password)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .process_group(0)
        .spawn()
        .map_err(|e| e.to_string())?;
    let _ = child.stdin.take().map(|mut stdin| stdin.write_all(public));
    let stderr = child.stderr.take().map(|mut pipe| {
        std::thread::spawn(move || {
            let mut text = String::new();
            let _ = pipe.read_to_string(&mut text);
            text
        })
    });
    let deadline = Instant::now() + SIGN_IN_TIMEOUT;
    let status = loop {
        match child.try_wait().map_err(|e| e.to_string())? {
            Some(status) => break status,
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                // ssh's own wording, so `classify` calls it TimedOut.
                return Err("Operation timed out".into());
            }
            None => std::thread::sleep(Duration::from_millis(50)),
        }
    };
    let stderr = stderr.and_then(|reader| reader.join().ok()).unwrap_or_default();
    if status.success() { Ok(()) } else { Err(stderr) }
}

pub fn remove_key(host: &Host, home: &Path) {
    let key = host.app_key(home);
    let _ = std::fs::remove_file(public_key(&key));
    let _ = std::fs::remove_file(key);
}

fn public_key(key: &Path) -> PathBuf {
    let mut public = key.as_os_str().to_owned();
    public.push(".pub");
    public.into()
}

fn keygen(key: &Path) -> Result<(), Failure> {
    let out = Command::new("ssh-keygen").args(["-q", "-t", "ed25519", "-N", "", "-C", "anywhere", "-f"]).arg(key).stdin(Stdio::null()).output();
    out.ok().filter(|o| o.status.success()).map(|_| ()).ok_or(Failure::Other)
}
