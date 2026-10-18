use super::Service;
use serde::Deserialize;
use std::path::PathBuf;

/// What `pocketd service … --json` prints.
#[derive(Deserialize)]
struct State {
    installed: bool,
}

/// The pocketd the release package ships beside the app's own binary.
fn pocketd() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let path = exe.with_file_name(if cfg!(windows) { "pocketd.exe" } else { "pocketd" });
    if path.is_file() { Ok(path) } else { Err(format!("{} is missing", path.display())) }
}

fn run(program: impl AsRef<std::ffi::OsStr>, args: &[&str]) -> Result<String, String> {
    let mut cmd = crate::command(program);
    cmd.args(args);
    let out = cmd.output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Runs `pocketd service <action>` for the app's own label, apart from a host install's.
fn service(action: &str) -> Result<State, String> {
    let out = run(pocketd()?, &["service", action, "--json", "--label", channel::daemon_label()])?;
    out.lines().rev().find_map(|l| serde_json::from_str(l).ok()).ok_or_else(|| format!("pocketd service {action}: {out}"))
}

/// The owner token this app sends its own pocketd, minted once with `pocketd pair` and kept in `file`.
#[cfg(windows)]
pub fn own_token(file: &std::path::Path) -> Option<String> {
    if let Ok(token) = std::fs::read_to_string(file).map(|t| t.trim().to_string())
        && !token.is_empty()
    {
        return Some(token);
    }
    // A dev build has no pocketd beside it; the one on PATH is the one running.
    let out = run(pocketd().unwrap_or_else(|_| "pocketd".into()), &["pair", "--json", "--name", "Anywhere desktop"]).ok()?;
    let token = out.lines().rev().find_map(|l| serde_json::from_str::<serde_json::Value>(l).ok()?["token"].as_str().map(String::from))?;
    std::fs::write(file, &token).ok()?;
    Some(token)
}

pub fn status() -> Service {
    match service("status") {
        Ok(s) if s.installed => Service::Enabled,
        Ok(_) => Service::NotRegistered,
        Err(_) => Service::NotFound,
    }
}

pub fn register() -> Result<(), String> {
    service("install").map(|_| ())
}

/// Restarts the service; pocketd resumes its agents on the way back.
pub fn restart() -> Result<(), String> {
    let label = channel::daemon_label();
    #[cfg(windows)]
    {
        let _ = run("schtasks", &["/End", "/TN", label]);
        run("schtasks", &["/Run", "/TN", label]).map(|_| ())
    }
    #[cfg(not(windows))]
    run("systemctl", &["--user", "restart", &format!("{label}.service")]).map(|_| ())
}

/// Makes the app itself open at login, or not: an XDG autostart entry, or the Run key on Windows.
pub fn open_at_login(on: bool) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    #[cfg(windows)]
    {
        let key = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
        let name = channel::bundle_id();
        let result = if on {
            run("reg", &["add", key, "/v", name, "/t", "REG_SZ", "/d", &format!("\"{}\"", exe.display()), "/f"])
        } else {
            run("reg", &["delete", key, "/v", name, "/f"])
        };
        // Deleting a value that isn't there fails, and leaves what was asked for.
        result.map(|_| ()).or_else(|e| if on { Err(e) } else { Ok(()) })
    }
    #[cfg(not(windows))]
    {
        let dir = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .ok_or("HOME isn't set")?
            .join("autostart");
        let path = dir.join(format!("{}.desktop", channel::bundle_id()));
        if !on {
            return match std::fs::remove_file(&path) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
                _ => Ok(()),
            };
        }
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        std::fs::write(&path, autostart_entry(&exe.to_string_lossy())).map_err(|e| e.to_string())
    }
}

/// Only macOS can switch the service off behind the app's back, so there is nothing to open.
pub fn open_login_items() {}

#[cfg(not(windows))]
fn autostart_entry(exe: &str) -> String {
    // The Exec key quotes with "…" and escapes ", `, $ and \ inside them.
    let exe: String = exe.chars().flat_map(|c| if matches!(c, '"' | '`' | '$' | '\\') { vec!['\\', c] } else { vec![c] }).collect();
    format!("[Desktop Entry]\nType=Application\nName=Anywhere\nExec=\"{exe}\"\nX-GNOME-Autostart-enabled=true\n")
}

#[cfg(all(test, not(windows)))]
mod tests {
    use super::autostart_entry;

    #[test]
    fn the_autostart_entry_quotes_the_app_path() {
        assert!(autostart_entry("/opt/Any where/anywhere").contains("Exec=\"/opt/Any where/anywhere\"\n"));
        assert!(autostart_entry("/home/a$b/x").contains("Exec=\"/home/a\\$b/x\"\n"));
    }
}
