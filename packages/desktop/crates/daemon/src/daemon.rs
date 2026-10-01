use base64::{Engine, engine::general_purpose::STANDARD};
use futures::channel::mpsc::{UnboundedReceiver, unbounded};
use serde::Deserialize;
use serde_json::{Value, json};
use std::ffi::CStr;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

#[derive(Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Info {
    pub id: String,
    pub cmd: String,
    pub args: Vec<String>,
    pub cwd: String,
    pub foreground: String,
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
}

impl Msg {
    pub fn bytes(&self) -> Vec<u8> {
        self.data.as_deref().and_then(|d| STANDARD.decode(d).ok()).unwrap_or_default()
    }
}

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
        .unwrap_or_else(|| PathBuf::from(env("HOME").unwrap_or_default()).join(".coding-pocket"));
    home.join("pocketd.sock")
}

fn resolve_cwd(cwd: &str, home: &str) -> String {
    match cwd.trim() {
        "" | "~" => home.to_string(),
        c => c.strip_prefix("~/").map_or_else(|| c.to_string(), |rest| format!("{home}/{rest}")),
    }
}

/// The shell on the user's account, set with `chsh`. `$SHELL` is whatever the app was launched from.
pub fn login_shell() -> &'static str {
    static SHELL: OnceLock<String> = OnceLock::new();
    SHELL.get_or_init(|| {
        let pw = unsafe { libc::getpwuid(libc::getuid()) };
        let shell = (!pw.is_null()).then(|| unsafe { (*pw).pw_shell }).filter(|s| !s.is_null());
        shell.map(|s| unsafe { CStr::from_ptr(s) }.to_string_lossy().into_owned()).filter(|s| !s.is_empty()).unwrap_or_else(|| "/bin/zsh".into())
    })
}

/// A new terminal's env, as Terminal.app gives it: the account's basics, nothing from the terminal that launched the app.
/// The login shell builds the rest.
pub fn terminal_env() -> Vec<(String, String)> {
    terminal_env_in(|k| std::env::var(k).ok().filter(|v| !v.is_empty()), login_shell())
}

fn terminal_env_in(env: impl Fn(&str) -> Option<String>, shell: &str) -> Vec<(String, String)> {
    let kept = ["HOME", "USER", "LOGNAME", "TMPDIR", "SSH_AUTH_SOCK", "__CF_USER_TEXT_ENCODING"].into_iter().filter_map(|k| Some((k.to_string(), env(k)?)));
    let set = [
        ("LANG", env("LANG").unwrap_or_else(|| "en_US.UTF-8".into())),
        ("PATH", "/usr/bin:/bin:/usr/sbin:/sbin".into()),
        ("SHELL", shell.into()),
        ("TERM", "xterm-256color".into()),
        ("COLORTERM", "truecolor".into()),
        ("TERM_PROGRAM", "Pocket".into()),
    ];
    kept.chain(set.map(|(k, v)| (k.to_string(), v))).collect()
}

pub fn shell_op(cwd: &str) -> Value {
    spawn_op(login_shell(), vec!["-l".into()], cwd)
}

/// Runs an agent inside the login shell, which takes over once the agent exits, so the user lands at their prompt.
pub fn agent_op(argv: &[String], cwd: &str) -> Value {
    spawn_op(login_shell(), agent_args(login_shell(), "", argv), cwd)
}

/// Runs `setup` in the terminal first, where the user can watch it; the agent starts only if it succeeds.
pub fn setup_op(setup: &str, argv: &[String], cwd: &str) -> Value {
    spawn_op(login_shell(), agent_args(login_shell(), setup, argv), cwd)
}

/// The agent's argv goes to the shell as arguments rather than inside the script, so no shell's quoting rules can garble a prompt.
fn agent_args(shell: &str, setup: &str, argv: &[String]) -> Vec<String> {
    // On its own lines, so a `#` comment or trailing separator in the setup can't swallow the rest.
    let first = |open: &str, close: &str| if setup.is_empty() { String::new() } else { format!("{open}\n{setup}\n{close} || exit\n") };
    let back = format!("exec '{shell}' -l");
    let mut args = vec!["-l".to_string(), "-c".to_string()];
    if Path::new(shell).file_name().is_some_and(|n| n == "fish") {
        args.push(format!("{}$argv; {back}", first("begin", "end")));
    } else {
        // `sh -c` binds the first argument after the script to $0.
        args.extend([format!("{}\"$@\"; {back}", first("{", "}")), shell.to_string()]);
    }
    args.extend(argv.iter().cloned());
    args
}

/// Runs `argv` in `cwd` under the login shell, so it finds the tools the user's terminal would, with `input` on stdin.
/// Its output, or on failure what it printed.
pub fn run_login(argv: &[&str], cwd: &str, input: &str) -> Result<String, String> {
    run_in(login_shell(), terminal_env(), argv, cwd, input)
}

fn run_in(shell: &str, env: Vec<(String, String)>, argv: &[&str], cwd: &str, input: &str) -> Result<String, String> {
    let fish = Path::new(shell).file_name().is_some_and(|n| n == "fish");
    let mut cmd = Command::new(shell);
    cmd.args(["-l", "-c", if fish { "$argv" } else { "\"$@\"" }]);
    if !fish {
        cmd.arg(shell);
    }
    let mut child = cmd
        .args(argv)
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

fn spawn_op(cmd: &str, args: Vec<String>, cwd: &str) -> Value {
    let env: Vec<String> = terminal_env().into_iter().map(|(k, v)| format!("{k}={v}")).collect();
    let cwd = resolve_cwd(cwd, &std::env::var("HOME").unwrap_or_default());
    json!({"op": "spawn", "cmd": cmd, "args": args, "cwd": cwd, "env": env, "cols": 120, "rows": 36})
}

#[derive(Clone)]
pub struct Daemon {
    writer: Arc<Mutex<Option<UnixStream>>>,
}

impl Daemon {
    /// Connects in the background and keeps reconnecting, reporting each change as a `Msg` with `ev` "up" or "down".
    pub fn spawn(path: &Path) -> (Self, UnboundedReceiver<Msg>) {
        let path = path.to_path_buf();
        let writer = Arc::new(Mutex::new(None));
        let (tx, rx) = unbounded();
        let shared = writer.clone();
        std::thread::spawn(move || {
            let link = |ev: &str| tx.unbounded_send(Msg { ev: ev.into(), ..Default::default() }).is_ok();
            let mut wait = Duration::from_millis(200);
            let mut down = false;
            loop {
                if let Ok((reader, stream)) = UnixStream::connect(&path).and_then(|s| Ok((BufReader::new(s.try_clone()?), s))) {
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
    use std::os::unix::net::UnixListener;

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
        assert_eq!(sock_path_in(env(&[("POCKETD_SOCK", ""), ("POCKET_HOME", ""), ("HOME", "/h")])), PathBuf::from("/h/.coding-pocket/pocketd.sock"));
        assert_eq!(sock_path_in(env(&[("POCKET_HOME", "/p"), ("HOME", "/h")])), PathBuf::from("/p/pocketd.sock"));
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
    fn shells_run_in_the_login_shell() {
        let op = shell_op("/w");
        assert_eq!(op["cmd"], login_shell());
        assert_eq!(op["args"], json!(["-l"]));
        assert_eq!(op["cwd"], "/w");
    }

    #[test]
    fn agents_hand_their_argv_to_the_shell_untouched() {
        let argv: Vec<String> = ["claude", "it's a \\ \"prompt\"\n"].map(String::from).to_vec();
        assert_eq!(agent_args("/opt/homebrew/bin/fish", "", &argv), ["-l", "-c", "$argv; exec '/opt/homebrew/bin/fish' -l", "claude", "it's a \\ \"prompt\"\n"]);
        assert_eq!(agent_args("/bin/zsh", "", &argv), ["-l", "-c", "\"$@\"; exec '/bin/zsh' -l", "/bin/zsh", "claude", "it's a \\ \"prompt\"\n"]);
    }

    #[test]
    fn agents_come_back_to_the_prompt_in_real_shells() {
        for shell in ["/bin/zsh", "/bin/bash", "/bin/sh", "/opt/homebrew/bin/fish"].into_iter().filter(|s| Path::new(s).exists()) {
            let argv = ["printf", "[%s]", "it's a \\ \"prompt\""].map(String::from);
            let mut args = agent_args(shell, "", &argv);
            args[2] = args[2].replace("exec", "echo");
            let home = std::env::temp_dir().join("pocket-desktop-no-home");
            let out = std::process::Command::new(shell).args(&args).env_clear().env("HOME", &home).env("PATH", "/usr/bin:/bin").output().unwrap();
            assert_eq!(String::from_utf8_lossy(&out.stdout), format!("[it's a \\ \"prompt\"]{shell} -l\n"), "{shell}");
        }
    }

    #[test]
    fn runs_argv_in_real_shells_with_stdin() {
        for shell in ["/bin/zsh", "/bin/bash", "/bin/sh", "/opt/homebrew/bin/fish"].into_iter().filter(|s| Path::new(s).exists()) {
            let home = std::env::temp_dir().join("pocket-desktop-no-home");
            let env = vec![("HOME".to_string(), home.to_string_lossy().into_owned()), ("PATH".to_string(), "/usr/bin:/bin".to_string())];
            let ok = run_in(shell, env.clone(), &["sh", "-c", "cat; printf '[%s]' \"$1\"", "sh", "it's \"x\""], "/", "in\n");
            assert_eq!(ok, Ok("in\n[it's \"x\"]".to_string()), "{shell}");
            assert_eq!(run_in(shell, env, &["sh", "-c", "echo out; echo oops >&2; exit 3"], "/", ""), Err("oops".to_string()), "{shell}");
        }
    }

    #[test]
    fn setup_runs_first_and_a_failed_one_stops_the_agent() {
        for shell in ["/bin/zsh", "/bin/bash", "/bin/sh", "/opt/homebrew/bin/fish"].into_iter().filter(|s| Path::new(s).exists()) {
            let argv = ["printf", "[%s]", "agent"].map(String::from);
            let run = |setup: &str| {
                let mut args = agent_args(shell, setup, &argv);
                args[2] = args[2].replace("exec", "echo");
                let home = std::env::temp_dir().join("pocket-desktop-no-home");
                std::process::Command::new(shell).args(&args).env_clear().env("HOME", &home).env("PATH", "/usr/bin:/bin").output().unwrap()
            };
            for setup in ["printf ready", "printf ready # note", "printf ready;", "printf ready\n"] {
                let ok = run(setup);
                assert_eq!(String::from_utf8_lossy(&ok.stdout), format!("ready[agent]{shell} -l\n"), "{shell}: {setup:?}");
            }
            let failed = run("sh -c 'exit 7'");
            assert_eq!(failed.status.code(), Some(7), "{shell}");
            assert_eq!(String::from_utf8_lossy(&failed.stdout), "", "{shell}");
        }
    }

    #[test]
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

    #[test]
    fn send_returns_false_while_down() {
        let path = sock("down");
        let (d, mut rx) = Daemon::spawn(&path);
        assert_eq!(next(&mut rx), "down");
        assert!(!d.send(json!({"op": "list"})));
        assert!(!d.input("s1", b"x"));
    }
}
