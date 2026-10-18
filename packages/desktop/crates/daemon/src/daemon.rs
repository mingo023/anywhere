use base64::{Engine, engine::general_purpose::STANDARD};
use futures::channel::mpsc::{UnboundedReceiver, unbounded};
use serde::Deserialize;
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
#[cfg(unix)]
pub use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
#[cfg(windows)]
pub use uds_windows::UnixStream;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

pub mod service;

#[derive(Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Info {
    pub id: String,
    pub cmd: String,
    pub args: Vec<String>,
    pub cwd: String,
    pub foreground: String,
    /// The Local the terminal was opened in; empty when it belongs to the worktree holding `cwd`.
    pub local: String,
}

#[derive(Deserialize, Default, Debug)]
#[serde(default)]
pub struct Msg {
    pub ev: String,
    pub id: String,
    pub cols: u16,
    pub rows: u16,
    pub code: i32,
    pub data: Option<String>,
    pub items: Vec<Info>,
    pub error: String,
    pub text: String,
    pub status: Option<Status>,
    pub devices: Vec<Device>,
    pub accounts: Vec<Account>,
}

/// The part of pocketd's `status` reply Settings reads.
#[derive(Deserialize, Default, Debug, Clone, PartialEq)]
#[serde(default)]
pub struct Status {
    /// Every `config-set` key with its value; absent from a pocketd too old to take them.
    pub config: Option<serde_json::Map<String, Value>>,
    pub version: String,
    /// Seconds.
    pub uptime: u64,
    /// The phone listener's addresses, "100.64.0.1:4517".
    pub listen: Vec<String>,
    /// Each built-in agent's path, empty when it isn't on the login PATH.
    pub providers: std::collections::BTreeMap<String, String>,
}

/// A paired phone, as pocketd's `devices` op lists it.
#[derive(Deserialize, Default, Debug, Clone, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Device {
    pub id: String,
    pub name: String,
    pub platform: String,
    /// Milliseconds since the epoch; 0 before it first connects.
    pub last_seen_at: i64,
    /// The shared token from before per-device pairing; it can't be renamed or revoked.
    pub legacy: bool,
}

/// A Claude Code or Codex sign-in, as pocketd's `accounts` op lists it.
#[derive(Deserialize, Default, Debug, Clone, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Account {
    /// `builtin` for the login in `~/.claude` or `~/.codex`.
    pub id: String,
    pub provider: String,
    /// Empty shows `email`.
    pub label: String,
    pub email: String,
    pub plan: String,
    pub signed_in: bool,
    /// What new sessions of its provider start with.
    pub default: bool,
    pub running: usize,
    pub usage: Option<Usage>,
    /// Signing in; `error` says why that failed.
    pub pending: bool,
    pub error: String,
}

/// An account's 5-hour and weekly limits as of `at`; stale ones are the last known.
#[derive(Deserialize, Default, Debug, Clone, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Usage {
    pub session: Option<Window>,
    pub week: Option<Window>,
    /// Milliseconds since the epoch.
    pub at: i64,
    pub stale: bool,
}

#[derive(Deserialize, Default, Debug, Clone, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Window {
    pub percent: f64,
    /// Milliseconds since the epoch; 0 when the provider gave none.
    pub resets_at: i64,
}

/// Sends one op on its own connection and returns pocketd's reply, so it never reaches the terminal stream.
/// Blocks for up to five seconds; call it off the UI thread.
pub fn request(msg: &Value) -> Result<Msg, String> {
    request_to(&Endpoint::own(), msg, Duration::from_secs(5))
}

/// What `request_at` returns when the socket closes before pocketd replies.
pub const CLOSED: &str = "pocketd closed the connection";

/// `request` to the pocketd listening on `path`.
pub fn request_at(path: &Path, msg: &Value) -> Result<Msg, String> {
    request_at_with_timeout(path, msg, Duration::from_secs(5))
}

/// `request_at`, waiting up to `timeout` for the reply.
pub fn request_at_with_timeout(path: &Path, msg: &Value, timeout: Duration) -> Result<Msg, String> {
    request_to(&Endpoint::local(path), msg, timeout)
}

/// What pocketd answers an auth line whose token it doesn't know.
pub const NOT_OWNER: &str = "not an owner token";

/// Where a pocketd listens: a socket, and the owner token it wants first when the socket is a
/// Windows host's TCP port forwarded by ssh.
#[derive(Clone, Debug, PartialEq)]
pub struct Endpoint {
    pub path: PathBuf,
    pub token: Option<String>,
}

impl Endpoint {
    pub fn local(path: &Path) -> Self {
        Self { path: path.into(), token: None }
    }

    /// This machine's pocketd. On Windows it can't tell the app from a process inside a Terminal, so the app proves itself with a token.
    pub fn own() -> Self {
        let path = sock_path();
        #[cfg(windows)]
        let token = service::own_token(&path.with_file_name("desktop.token"));
        #[cfg(not(windows))]
        let token = None;
        Self { path, token }
    }

    /// A connection pocketd treats as its owner. Without a token nothing is sent first.
    /// The reply to auth is read a byte at a time so nothing after it is lost to a buffer.
    #[allow(clippy::unbuffered_bytes)]
    pub fn connect(&self, timeout: Duration) -> Result<UnixStream, String> {
        let stream = UnixStream::connect(&self.path).map_err(|e| e.to_string())?;
        let Some(token) = &self.token else { return Ok(stream) };
        stream.set_read_timeout(Some(timeout)).map_err(|e| e.to_string())?;
        writeln!(&stream, "{}", json!({"op": "auth", "token": token})).map_err(|e| e.to_string())?;
        let mut line = Vec::new();
        for byte in (&stream).bytes() {
            match byte.map_err(|e| e.to_string())? {
                b'\n' => break,
                b => line.push(b),
            }
        }
        if line.is_empty() {
            return Err(CLOSED.into());
        }
        let reply: Msg = serde_json::from_slice(&line).map_err(|e| e.to_string())?;
        if reply.ev != "ok" {
            return Err(reply.error);
        }
        stream.set_read_timeout(None).map_err(|e| e.to_string())?;
        Ok(stream)
    }
}

/// `request_at_with_timeout` to `endpoint`.
pub fn request_to(endpoint: &Endpoint, msg: &Value, timeout: Duration) -> Result<Msg, String> {
    let stream = endpoint.connect(timeout)?;
    stream.set_read_timeout(Some(timeout)).map_err(|e| e.to_string())?;
    writeln!(&stream, "{msg}").map_err(|e| e.to_string())?;
    let mut line = String::new();
    if BufReader::new(&stream).read_line(&mut line).map_err(|e| e.to_string())? == 0 {
        return Err(CLOSED.into());
    }
    let reply: Msg = serde_json::from_str(&line).map_err(|e| e.to_string())?;
    if reply.ev == "error" {
        return Err(reply.error);
    }
    Ok(reply)
}

impl Msg {
    pub fn bytes(&self) -> Vec<u8> {
        self.data.as_deref().and_then(|d| STANDARD.decode(d).ok()).unwrap_or_default()
    }
}

/// The variable Go's `os.UserHomeDir` reads.
const HOME: &str = if cfg!(windows) { "USERPROFILE" } else { "HOME" };

/// Mirrors `config.Sock` in pocketd.
pub fn sock_path() -> PathBuf {
    sock_path_in(|k| std::env::var(k).ok())
}

fn sock_path_in(env: impl Fn(&str) -> Option<String>) -> PathBuf {
    let env = |k| env(k).filter(|v| !v.is_empty());
    if let Some(s) = env("POCKETD_SOCK") {
        return s.into();
    }
    let home = env("POCKET_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env(HOME).unwrap_or_default()).join(channel::home_dir_name()));
    home.join("pocketd.sock")
}

fn resolve_cwd(cwd: &str, home: &str) -> String {
    match cwd.trim() {
        "" | "~" => home.to_string(),
        c => c.strip_prefix("~/").map_or_else(|| c.to_string(), |rest| format!("{home}/{rest}")),
    }
}

/// The shell on the user's account, set with `chsh`. `$SHELL` is whatever the app was launched from.
#[cfg(unix)]
pub fn login_shell() -> &'static str {
    static SHELL: OnceLock<String> = OnceLock::new();
    SHELL.get_or_init(|| {
        let pw = unsafe { libc::getpwuid(libc::getuid()) };
        let shell = (!pw.is_null()).then(|| unsafe { (*pw).pw_shell }).filter(|s| !s.is_null());
        first_that_runs(shell.map(|s| unsafe { std::ffi::CStr::from_ptr(s) }.to_string_lossy().into_owned()), runs)
    })
}

/// PowerShell 7 when installed, else the Windows PowerShell every Windows has.
#[cfg(windows)]
pub fn login_shell() -> &'static str {
    static SHELL: OnceLock<String> = OnceLock::new();
    SHELL.get_or_init(|| {
        let env = |k: &str| std::env::var(k).unwrap_or_default();
        let pwsh = format!(r"{}\PowerShell\7\pwsh.exe", env("ProgramFiles"));
        let windows = format!(r"{}\System32\WindowsPowerShell\v1.0\powershell.exe", env("SystemRoot"));
        [pwsh, windows].into_iter().find(|s| runs(s)).unwrap_or_else(|| "powershell.exe".into())
    })
}

/// The account's shell, or macOS's own when it's unset or was uninstalled after `chsh`.
#[cfg(unix)]
fn first_that_runs(account: Option<String>, can_run: impl Fn(&str) -> bool) -> String {
    account.into_iter().chain(["/bin/zsh", "/bin/bash"].map(String::from)).find(|s| can_run(s)).unwrap_or_else(|| "/bin/sh".into())
}

/// An absolute path to a file someone may execute.
pub fn runs(path: &str) -> bool {
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    #[cfg(unix)]
    let executable = |m: &std::fs::Metadata| m.permissions().mode() & 0o111 != 0;
    #[cfg(windows)]
    let executable = |_: &std::fs::Metadata| true;
    Path::new(path).is_absolute() && std::fs::metadata(path).is_ok_and(|m| m.is_file() && executable(&m))
}

/// A new terminal's env, as Terminal.app gives it: the account's basics, nothing from the terminal that launched the app.
/// The login shell builds the rest.
pub fn terminal_env() -> Vec<(String, String)> {
    shell_env(login_shell())
}

#[cfg(unix)]
fn shell_env(shell: &str) -> Vec<(String, String)> {
    terminal_env_in(|k| std::env::var(k).ok().filter(|v| !v.is_empty()), shell)
}

/// Windows programs need most of the app's env (SystemRoot, APPDATA, …), and no login shell rebuilds it.
#[cfg(windows)]
fn shell_env(shell: &str) -> Vec<(String, String)> {
    let set = [("SHELL", shell), ("TERM", "xterm-256color"), ("COLORTERM", "truecolor"), ("TERM_PROGRAM", "ghostty")];
    let kept = std::env::vars().filter(|(k, _)| !set.iter().any(|(s, _)| k.eq_ignore_ascii_case(s)));
    kept.chain(set.map(|(k, v)| (k.to_string(), v.to_string()))).collect()
}

#[cfg(unix)]
fn terminal_env_in(env: impl Fn(&str) -> Option<String>, shell: &str) -> Vec<(String, String)> {
    let kept = ["HOME", "USER", "LOGNAME", "TMPDIR", "SSH_AUTH_SOCK", "__CF_USER_TEXT_ENCODING"].into_iter().filter_map(|k| Some((k.to_string(), env(k)?)));
    let set = [
        ("LANG", env("LANG").unwrap_or_else(|| "en_US.UTF-8".into())),
        ("PATH", "/usr/bin:/bin:/usr/sbin:/sbin".into()),
        ("SHELL", shell.into()),
        ("TERM", "xterm-256color".into()),
        ("COLORTERM", "truecolor".into()),
        // Lets Claude Code open links on click: mouse reports can't carry ⌘.
        ("TERM_PROGRAM", "ghostty".into()),
    ];
    kept.chain(set.map(|(k, v)| (k.to_string(), v))).collect()
}

/// Opens `shell` with `args`, which a new terminal's `$SHELL` names.
pub fn shell_op(shell: &str, args: Vec<String>, cwd: &str) -> Value {
    spawn_op(shell, args, shell_env(shell), cwd)
}

/// Runs an agent inside the login shell, which takes over once the agent exits, so the user lands at their prompt.
pub fn agent_op(argv: &[String], cwd: &str) -> Value {
    spawn_op(login_shell(), agent_args(login_shell(), argv), terminal_env(), cwd)
}

/// The agent's argv goes to the shell as arguments rather than inside the script, so no shell's quoting rules can garble a prompt.
#[cfg(unix)]
fn agent_args(shell: &str, argv: &[String]) -> Vec<String> {
    let back = format!("exec '{shell}' -l");
    let mut args = vec!["-l".to_string(), "-c".to_string()];
    if Path::new(shell).file_name().is_some_and(|n| n == "fish") {
        args.push(format!("$argv; {back}"));
    } else {
        // `sh -c` binds the first argument after the script to $0.
        args.extend([format!("\"$@\"; {back}"), shell.to_string()]);
    }
    args.extend(argv.iter().cloned());
    args
}

/// PowerShell takes no script arguments, so each of argv is a single-quoted string, in which only `'` is special.
#[cfg(windows)]
fn agent_args(_shell: &str, argv: &[String]) -> Vec<String> {
    let quoted: Vec<String> = argv.iter().map(|a| format!("'{}'", a.replace('\'', "''"))).collect();
    ["-NoLogo", "-NoExit", "-Command"].map(String::from).into_iter().chain([format!("& {}", quoted.join(" "))]).collect()
}

/// Runs `argv` in `cwd` under the login shell, so it finds the tools the user's terminal would, with `input` on stdin.
/// Its output, or on failure what it printed.
pub fn run_login(argv: &[&str], cwd: &str, input: &str) -> Result<String, String> {
    run_login_env(argv, cwd, input, Vec::new())
}

/// [`run_login`] with `extra` added to the environment.
pub fn run_login_env(argv: &[&str], cwd: &str, input: &str, extra: Vec<(String, String)>) -> Result<String, String> {
    let mut env = terminal_env();
    env.extend(extra);
    run_in(login_shell(), env, argv, cwd, input)
}

/// `argv` under the login shell `shell`.
#[cfg(unix)]
fn login_command(shell: &str, argv: &[&str]) -> Command {
    let fish = Path::new(shell).file_name().is_some_and(|n| n == "fish");
    let mut cmd = Command::new(shell);
    cmd.args(["-l", "-c", if fish { "$argv" } else { "\"$@\"" }]);
    if !fish {
        cmd.arg(shell);
    }
    cmd.args(argv);
    cmd
}

/// `argv` itself: on Windows the app's own PATH is the user's.
#[cfg(windows)]
fn login_command(_shell: &str, argv: &[&str]) -> Command {
    let mut cmd = command(argv[0]);
    cmd.args(&argv[1..]);
    cmd
}

#[cfg(not(windows))]
pub fn command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    Command::new(program)
}

/// `Command::new`, minus the console window Windows gives a console program the app starts.
#[cfg(windows)]
pub fn command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut cmd = Command::new(program);
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

fn run_in(shell: &str, env: Vec<(String, String)>, argv: &[&str], cwd: &str, input: &str) -> Result<String, String> {
    let mut child = login_command(shell, argv)
        .current_dir(cwd)
        .env_clear()
        .envs(env)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let mut stdin = child.stdin.take().expect("stdin is piped");
    let input = input.to_string();
    // Written from its own thread: a child that prints before reading all of it would fill its pipe while this one waits.
    std::thread::spawn(move || stdin.write_all(input.as_bytes()));
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    let text = |b: &[u8]| String::from_utf8_lossy(b).trim().to_string();
    if out.status.success() {
        Ok(text(&out.stdout))
    } else {
        Err(Some(text(&out.stderr)).filter(|e| !e.is_empty()).unwrap_or_else(|| text(&out.stdout)))
    }
}

/// Runs `script` in `cwd` under the login shell with `env` added, in its own process group so a timeout kills whatever it started too.
/// Its output, or on failure its last 20 lines.
pub fn run_script(script: &str, cwd: &str, env: &[(String, String)], timeout: Duration) -> Result<String, String> {
    run_script_in(login_shell(), [terminal_env(), env.to_vec()].concat(), script, cwd, timeout)
}

fn run_script_in(shell: &str, env: Vec<(String, String)>, script: &str, cwd: &str, timeout: Duration) -> Result<String, String> {
    let (mut out, writer) = std::io::pipe().map_err(|e| e.to_string())?;
    let mut cmd = command(shell);
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(cmd.args(["-l", "-c", script]), 0);
    #[cfg(windows)]
    cmd.args(["-NoLogo", "-Command", script]);
    let mut child = cmd
        .current_dir(cwd)
        .env_clear()
        .envs(env)
        .stdin(Stdio::null())
        .stdout(writer.try_clone().map_err(|e| e.to_string())?)
        .stderr(writer)
        .spawn()
        .map_err(|e| e.to_string())?;
    // It still holds the pipe's write end, and the reader sees EOF only once every copy is closed.
    drop(cmd);
    let (tx, rx) = std::sync::mpsc::channel();
    // Read on its own thread: a script that prints more than the pipe holds blocks until someone reads it.
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = out.read_to_end(&mut buf);
        let _ = tx.send(buf);
    });
    // The pipe closes once the script and everything it started have exited.
    let Ok(buf) = rx.recv_timeout(timeout) else {
        kill_tree(&child);
        let _ = child.wait();
        return Err(format!("Timed out after {timeout:?}"));
    };
    let status = child.wait().map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&buf).trim().to_string();
    if status.success() {
        return Ok(text);
    }
    let lines: Vec<&str> = text.lines().collect();
    let tail = lines[lines.len().saturating_sub(20)..].join("\n");
    Err(if tail.is_empty() { status.to_string() } else { tail })
}

/// Kills `child` and whatever it started.
fn kill_tree(child: &Child) {
    #[cfg(unix)]
    unsafe {
        libc::kill(-(child.id() as i32), libc::SIGKILL)
    };
    #[cfg(windows)]
    let _ = command("taskkill").args(["/T", "/F", "/PID", &child.id().to_string()]).status();
}

fn spawn_op(cmd: &str, args: Vec<String>, env: Vec<(String, String)>, cwd: &str) -> Value {
    let env: Vec<String> = env.into_iter().map(|(k, v)| format!("{k}={v}")).collect();
    let cwd = resolve_cwd(cwd, &std::env::var(HOME).unwrap_or_default());
    json!({"op": "spawn", "cmd": cmd, "args": args, "cwd": cwd, "env": env, "cols": 120, "rows": 36})
}

#[derive(Clone)]
pub struct Daemon {
    writer: Arc<Mutex<Option<UnixStream>>>,
}

impl Daemon {
    /// Connects in the background and keeps reconnecting, reporting each change as a `Msg` with `ev` "up" or "down".
    pub fn spawn(path: &Path) -> (Self, UnboundedReceiver<Msg>) {
        Self::spawn_to(Endpoint::local(path))
    }

    /// `spawn` to `endpoint`; a connection pocketd refuses as owner counts as down.
    pub fn spawn_to(endpoint: Endpoint) -> (Self, UnboundedReceiver<Msg>) {
        let writer = Arc::new(Mutex::new(None));
        let (tx, rx) = unbounded();
        let shared = writer.clone();
        std::thread::spawn(move || {
            let link = |ev: &str| tx.unbounded_send(Msg { ev: ev.into(), ..Default::default() }).is_ok();
            let mut wait = Duration::from_millis(200);
            let mut down = false;
            loop {
                if let Ok((reader, stream)) = endpoint.connect(Duration::from_secs(5)).and_then(|s| Ok((BufReader::new(s.try_clone().map_err(|e| e.to_string())?), s))) {
                    *shared.lock().unwrap() = Some(stream);
                    down = false;
                    if !link("up") {
                        return;
                    }
                    wait = Duration::from_millis(200);
                    for line in reader.lines().map_while(Result::ok) {
                        let Ok(msg) = serde_json::from_str::<Msg>(&line) else { continue };
                        if tx.unbounded_send(msg).is_err() {
                            return;
                        }
                    }
                    *shared.lock().unwrap() = None;
                }
                if !down {
                    if !link("down") {
                        return;
                    }
                    down = true;
                }
                if tx.is_closed() {
                    return;
                }
                std::thread::sleep(wait);
                wait = (wait * 2).min(Duration::from_secs(2));
            }
        });
        (Self { writer }, rx)
    }

    /// Whether the message reached pocketd's socket; false while it is down.
    pub fn send(&self, msg: Value) -> bool {
        self.writer.lock().unwrap().as_mut().is_some_and(|w| writeln!(w, "{msg}").is_ok())
    }

    pub fn input(&self, id: &str, bytes: &[u8]) -> bool {
        self.send(json!({"op": "input", "id": id, "data": STANDARD.encode(bytes)}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;
    use futures::executor::block_on;
    #[cfg(unix)]
    use std::os::unix::net::UnixListener;
    #[cfg(windows)]
    use uds_windows::UnixListener;

    #[test]
    #[cfg(unix)]
    fn an_uninstalled_login_shell_falls_back_to_one_that_runs() {
        let installed = |s: &str| s != "/opt/homebrew/bin/fish";
        assert_eq!(first_that_runs(Some("/opt/homebrew/bin/fish".into()), installed), "/bin/zsh");
        assert_eq!(first_that_runs(Some("/bin/bash".into()), installed), "/bin/bash");
        assert_eq!(first_that_runs(None, |s| s == "/bin/bash"), "/bin/bash");
        assert_eq!(first_that_runs(Some("".into()), |_| false), "/bin/sh");
    }

    #[test]
    #[cfg(unix)]
    fn only_an_absolute_executable_file_runs() {
        assert!(runs("/bin/sh"));
        assert!(!runs("sh") && !runs("/bin") && !runs("/etc/hosts") && !runs("/no/such/shell"));
    }

    #[test]
    fn a_status_from_an_older_pocketd_has_no_config() {
        let m: Msg = serde_json::from_str(r#"{"ev":"status","status":{"pid":1,"version":"0.4.0","uptime":90}}"#).unwrap();
        let s = m.status.unwrap();
        assert_eq!((s.config, s.version.as_str(), s.uptime), (None, "0.4.0", 90));
        let m: Msg = serde_json::from_str(r#"{"ev":"status","status":{"config":{"awake.enabled":false},"providers":{"claude":"/bin/claude"}}}"#).unwrap();
        let s = m.status.unwrap();
        assert_eq!((s.config.unwrap()["awake.enabled"].as_bool(), s.providers["claude"].as_str()), (Some(false), "/bin/claude"));
    }

    #[test]
    fn decodes_output_bytes() {
        let m: Msg = serde_json::from_str(r#"{"ev":"output","id":"s1","data":"aGk="}"#).unwrap();
        assert_eq!((m.ev.as_str(), m.id.as_str(), m.bytes()), ("output", "s1", b"hi".to_vec()));
    }

    #[test]
    fn decodes_session_list_ignoring_unknown_fields() {
        let m: Msg = serde_json::from_str(r#"{"ev":"terminals","items":[{"id":"a","cmd":"claude","args":["-c"],"cwd":"/w","cols":80,"rows":24}]}"#).unwrap();
        assert_eq!(m.items, vec![Info { id: "a".into(), cmd: "claude".into(), args: vec!["-c".into()], cwd: "/w".into(), ..Default::default() }]);
    }

    #[test]
    fn decodes_terminal_activity() {
        let m: Msg = serde_json::from_str(r#"{"ev":"terminals","items":[{"id":"a","cmd":"zsh","cwd":"/w","foreground":"npm run dev"}]}"#).unwrap();
        assert_eq!(m.items[0].foreground, "npm run dev");
        let m: Msg = serde_json::from_str(r#"{"ev":"foreground","id":"a","text":"cargo test"}"#).unwrap();
        assert_eq!(m.text, "cargo test");
    }

    #[test]
    fn empty_env_vars_count_as_unset() {
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |k: &str| pairs.iter().find(|(n, _)| *n == k).map(|(_, v)| v.to_string())
        };
        assert_eq!(
            sock_path_in(env(&[("POCKETD_SOCK", ""), ("POCKET_HOME", ""), (HOME, "/h")])),
            PathBuf::from("/h").join(channel::home_dir_name()).join("pocketd.sock")
        );
        assert_eq!(sock_path_in(env(&[("POCKET_HOME", "/p"), (HOME, "/h")])), PathBuf::from("/p/pocketd.sock"));
        assert_eq!(sock_path_in(env(&[("POCKETD_SOCK", "/s.sock")])), PathBuf::from("/s.sock"));
    }

    #[test]
    fn spawn_cwd_defaults_to_home_and_expands_tilde() {
        assert_eq!(resolve_cwd("  ", "/h"), "/h");
        assert_eq!(resolve_cwd("~", "/h"), "/h");
        assert_eq!(resolve_cwd("~/src", "/h"), "/h/src");
        assert_eq!(resolve_cwd("/w/~x", "/h"), "/w/~x");
    }

    #[test]
    fn a_shell_runs_with_its_arguments_and_names_itself_in_shell() {
        let op = shell_op("/opt/homebrew/bin/fish", vec!["--login".into()], "/w");
        assert_eq!(op["cmd"], "/opt/homebrew/bin/fish");
        assert_eq!(op["args"], json!(["--login"]));
        assert_eq!(op["cwd"], "/w");
        assert!(op["env"].as_array().unwrap().contains(&json!("SHELL=/opt/homebrew/bin/fish")));
    }

    #[test]
    #[cfg(unix)]
    fn agents_hand_their_argv_to_the_shell_untouched() {
        let argv: Vec<String> = ["claude", "it's a \\ \"prompt\"\n"].map(String::from).to_vec();
        assert_eq!(agent_args("/opt/homebrew/bin/fish", &argv), ["-l", "-c", "$argv; exec '/opt/homebrew/bin/fish' -l", "claude", "it's a \\ \"prompt\"\n"]);
        assert_eq!(agent_args("/bin/zsh", &argv), ["-l", "-c", "\"$@\"; exec '/bin/zsh' -l", "/bin/zsh", "claude", "it's a \\ \"prompt\"\n"]);
    }

    #[test]
    #[cfg(unix)]
    fn agents_come_back_to_the_prompt_in_real_shells() {
        for shell in ["/bin/zsh", "/bin/bash", "/bin/sh", "/opt/homebrew/bin/fish"].into_iter().filter(|s| Path::new(s).exists()) {
            let argv = ["printf", "[%s]", "it's a \\ \"prompt\""].map(String::from);
            let mut args = agent_args(shell, &argv);
            args[2] = args[2].replace("exec", "echo");
            let home = std::env::temp_dir().join("pocket-desktop-no-home");
            let out = std::process::Command::new(shell).args(&args).env_clear().env("HOME", &home).env("PATH", "/usr/bin:/bin").output().unwrap();
            assert_eq!(String::from_utf8_lossy(&out.stdout), format!("[it's a \\ \"prompt\"]{shell} -l\n"), "{shell}");
        }
    }

    #[test]
    #[cfg(unix)]
    fn runs_argv_in_real_shells_with_stdin() {
        for shell in ["/bin/zsh", "/bin/bash", "/bin/sh", "/opt/homebrew/bin/fish"].into_iter().filter(|s| Path::new(s).exists()) {
            let home = std::env::temp_dir().join("pocket-desktop-no-home");
            let env = vec![("HOME".to_string(), home.to_string_lossy().into_owned()), ("PATH".to_string(), "/usr/bin:/bin".to_string())];
            let ok = run_in(shell, env.clone(), &["sh", "-c", "cat; printf '[%s]' \"$1\"", "sh", "it's \"x\""], "/", "in\n");
            assert_eq!(ok, Ok("in\n[it's \"x\"]".to_string()), "{shell}");
            assert_eq!(run_in(shell, env, &["sh", "-c", "echo out; echo oops >&2; exit 3"], "/", ""), Err("oops".to_string()), "{shell}");
        }
    }

    #[cfg(unix)]
    fn bare_env() -> Vec<(String, String)> {
        let home = std::env::temp_dir().join("pocket-desktop-no-home");
        vec![("HOME".to_string(), home.to_string_lossy().into_owned()), ("PATH".to_string(), "/usr/bin:/bin".to_string())]
    }

    #[test]
    #[cfg(unix)]
    fn a_script_returns_its_output_and_on_failure_its_last_twenty_lines() {
        for shell in ["/bin/zsh", "/bin/bash", "/bin/sh", "/opt/homebrew/bin/fish"].into_iter().filter(|s| Path::new(s).exists()) {
            let wait = Duration::from_secs(5);
            assert_eq!(run_script_in(shell, bare_env(), "echo out; echo err >&2", "/", wait), Ok("out\nerr".to_string()), "{shell}");
            let tail: Vec<String> = (12..=30).map(|n| n.to_string()).chain(["oops".to_string()]).collect();
            assert_eq!(run_script_in(shell, bare_env(), "seq 30; echo oops >&2; exit 3", "/", wait), Err(tail.join("\n")), "{shell}");
            assert_eq!(run_script_in(shell, bare_env(), "exit 4", "/", wait), Err("exit status: 4".to_string()), "{shell}");
        }
    }

    #[test]
    #[cfg(unix)]
    fn a_script_that_runs_too_long_is_killed_with_what_it_started() {
        let marker = std::env::temp_dir().join(format!("pocket-script-{}", std::process::id()));
        let _ = std::fs::remove_file(&marker);
        let started = std::time::Instant::now();
        let script = format!("(sleep 1; touch '{}') & sleep 5", marker.display());
        assert_eq!(run_script_in("/bin/sh", bare_env(), &script, "/", Duration::from_millis(300)), Err("Timed out after 300ms".to_string()));
        assert!(started.elapsed() < Duration::from_secs(2));
        std::thread::sleep(Duration::from_millis(1200));
        assert!(!marker.exists(), "the background job outlived the timeout");
    }

    #[test]
    #[cfg(unix)]
    fn terminals_start_from_the_account_not_the_launching_terminal() {
        let launched = |k: &str| match k {
            "HOME" => Some("/h".to_string()),
            "USER" => Some("u".to_string()),
            "TERM" => Some("tmux-256color".to_string()),
            "TMUX" => Some("/tmp/tmux".to_string()),
            "PATH" => Some("/somewhere".to_string()),
            _ => None,
        };
        let env = terminal_env_in(launched, "/opt/homebrew/bin/fish");
        let get = |k: &str| env.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
        assert_eq!((get("HOME"), get("USER"), get("TMUX")), (Some("/h"), Some("u"), None));
        assert_eq!((get("SHELL"), get("TERM"), get("COLORTERM")), (Some("/opt/homebrew/bin/fish"), Some("xterm-256color"), Some("truecolor")));
        assert_eq!(get("TERM_PROGRAM"), Some("ghostty"));
        assert_eq!((get("PATH"), get("LANG")), (Some("/usr/bin:/bin:/usr/sbin:/sbin"), Some("en_US.UTF-8")));
    }

    fn sock(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pocket-desktop-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("d.sock")
    }

    fn next(rx: &mut UnboundedReceiver<Msg>) -> String {
        block_on(rx.next()).unwrap().ev
    }

    #[test]
    fn talks_json_lines_over_the_socket() {
        let path = sock("talk");
        let ln = UnixListener::bind(&path).unwrap();
        let (d, mut rx) = Daemon::spawn(&path);
        assert_eq!(next(&mut rx), "up");
        let (peer, _) = ln.accept().unwrap();

        assert!(d.input("s1", b"x"));
        let mut line = String::new();
        BufReader::new(peer.try_clone().unwrap()).read_line(&mut line).unwrap();
        assert_eq!(serde_json::from_str::<Value>(&line).unwrap(), json!({"op": "input", "id": "s1", "data": "eA=="}));

        writeln!(&peer, r#"{{"ev":"spawned","id":"s2"}}"#).unwrap();
        let m = block_on(rx.next()).unwrap();
        assert_eq!((m.ev.as_str(), m.id.as_str()), ("spawned", "s2"));
    }

    #[test]
    fn spawn_reports_down_then_up_when_the_socket_appears() {
        let path = sock("appear");
        let (_d, mut rx) = Daemon::spawn(&path);
        assert_eq!(next(&mut rx), "down");
        let _ln = UnixListener::bind(&path).unwrap();
        assert_eq!(next(&mut rx), "up");
    }

    #[test]
    fn spawn_reports_down_when_the_server_closes() {
        let path = sock("close");
        let ln = UnixListener::bind(&path).unwrap();
        let (_d, mut rx) = Daemon::spawn(&path);
        assert_eq!(next(&mut rx), "up");
        drop(ln.accept().unwrap());
        assert_eq!(next(&mut rx), "down");
    }

    fn read_line(peer: &UnixStream) -> Value {
        let mut line = String::new();
        BufReader::new(peer).read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap()
    }

    #[test]
    fn a_token_is_sent_first_and_what_pocketd_sends_right_after_ok_still_arrives() {
        let path = sock("auth");
        let ln = UnixListener::bind(&path).unwrap();
        let (d, mut rx) = Daemon::spawn_to(Endpoint { path, token: Some("t1".into()) });
        let (peer, _) = ln.accept().unwrap();
        assert_eq!(read_line(&peer), json!({"op": "auth", "token": "t1"}));
        write!(&peer, "{{\"ev\":\"ok\"}}\n{{\"ev\":\"spawned\",\"id\":\"s2\"}}\n").unwrap();
        assert_eq!(next(&mut rx), "up");
        assert_eq!(next(&mut rx), "spawned");
        assert!(d.send(json!({"op": "list"})));
        assert_eq!(read_line(&peer), json!({"op": "list"}));
    }

    #[test]
    fn without_a_token_the_first_line_is_the_request() {
        let path = sock("plain");
        let ln = UnixListener::bind(&path).unwrap();
        let server = std::thread::spawn(move || {
            let (peer, _) = ln.accept().unwrap();
            let first = read_line(&peer);
            writeln!(&peer, r#"{{"ev":"terminals"}}"#).unwrap();
            first
        });
        assert_eq!(request_at(&path, &json!({"op": "list"})).unwrap().ev, "terminals");
        assert_eq!(server.join().unwrap(), json!({"op": "list"}));
    }

    #[test]
    fn a_refused_token_fails_the_request_with_pocketds_words() {
        let path = sock("refused");
        let ln = UnixListener::bind(&path).unwrap();
        std::thread::spawn(move || {
            let (peer, _) = ln.accept().unwrap();
            read_line(&peer);
            writeln!(&peer, r#"{{"ev":"error","error":"not an owner token","errorCode":"not_paired"}}"#).unwrap();
        });
        let endpoint = Endpoint { path, token: Some("old".into()) };
        assert_eq!(request_to(&endpoint, &json!({"op": "status"}), Duration::from_secs(5)).unwrap_err(), NOT_OWNER);
    }

    #[test]
    fn send_returns_false_while_down() {
        let path = sock("down");
        let (d, mut rx) = Daemon::spawn(&path);
        assert_eq!(next(&mut rx), "down");
        assert!(!d.send(json!({"op": "list"})));
        assert!(!d.input("s1", b"x"));
    }
}
