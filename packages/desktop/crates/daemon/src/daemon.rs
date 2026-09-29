use base64::{Engine, engine::general_purpose::STANDARD};
use futures::channel::mpsc::{UnboundedReceiver, unbounded};
use serde::Deserialize;
use serde_json::{Value, json};
use std::ffi::CStr;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

#[derive(Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Info {
    pub id: String,
    pub cmd: String,
    pub args: Vec<String>,
    pub cwd: String,
    pub foreground: String,
    #[serde(rename = "lastProvider")]
    pub last_provider: String,
    #[serde(rename = "lastTitle")]
    pub last_title: String,
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
    spawn_op(login_shell(), agent_args(login_shell(), argv), cwd)
}

/// The agent's argv goes to the shell as arguments rather than inside the script, so no shell's quoting rules can garble a prompt.
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

fn spawn_op(cmd: &str, args: Vec<String>, cwd: &str) -> Value {
    let env: Vec<String> = terminal_env().into_iter().map(|(k, v)| format!("{k}={v}")).collect();
    let cwd = resolve_cwd(cwd, &std::env::var("HOME").unwrap_or_default());
    json!({"op": "spawn", "cmd": cmd, "args": args, "cwd": cwd, "env": env, "cols": 120, "rows": 36})
}

#[derive(Clone)]
pub struct Daemon {
    writer: Arc<Mutex<UnixStream>>,
}

impl Daemon {
    pub fn connect(path: &Path) -> std::io::Result<(Self, UnboundedReceiver<Msg>)> {
        let stream = UnixStream::connect(path)?;
        let reader = BufReader::new(stream.try_clone()?);
        let (tx, rx) = unbounded();
        std::thread::spawn(move || {
            for line in reader.lines().map_while(Result::ok) {
                let Ok(msg) = serde_json::from_str::<Msg>(&line) else { continue };
                if tx.unbounded_send(msg).is_err() {
                    return;
                }
            }
            let _ = tx.unbounded_send(Msg { ev: "error".into(), error: "pocketd disconnected".into(), ..Default::default() });
        });
        Ok((Self { writer: Arc::new(Mutex::new(stream)) }, rx))
    }

    pub fn send(&self, msg: Value) {
        let mut w = self.writer.lock().unwrap();
        let _ = writeln!(w, "{msg}");
    }

    pub fn input(&self, id: &str, bytes: &[u8]) {
        self.send(json!({"op": "input", "id": id, "data": STANDARD.encode(bytes)}));
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
        let m: Msg = serde_json::from_str(r#"{"ev":"terminals","items":[{"id":"a","cmd":"zsh","cwd":"/w","foreground":"npm run dev","lastProvider":"claude","lastTitle":"Fix CI"}]}"#).unwrap();
        let i = &m.items[0];
        assert_eq!((i.foreground.as_str(), i.last_provider.as_str(), i.last_title.as_str()), ("npm run dev", "claude", "Fix CI"));
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
        assert_eq!(agent_args("/opt/homebrew/bin/fish", &argv), ["-l", "-c", "$argv; exec '/opt/homebrew/bin/fish' -l", "claude", "it's a \\ \"prompt\"\n"]);
        assert_eq!(agent_args("/bin/zsh", &argv), ["-l", "-c", "\"$@\"; exec '/bin/zsh' -l", "/bin/zsh", "claude", "it's a \\ \"prompt\"\n"]);
    }

    #[test]
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

    #[test]
    fn talks_json_lines_over_the_socket() {
        let dir = std::env::temp_dir().join(format!("pocket-desktop-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("d.sock");
        let _ = std::fs::remove_file(&path);
        let ln = UnixListener::bind(&path).unwrap();
        let (d, mut rx) = Daemon::connect(&path).unwrap();
        let (peer, _) = ln.accept().unwrap();

        d.input("s1", b"x");
        let mut line = String::new();
        BufReader::new(peer.try_clone().unwrap()).read_line(&mut line).unwrap();
        assert_eq!(serde_json::from_str::<Value>(&line).unwrap(), json!({"op": "input", "id": "s1", "data": "eA=="}));

        writeln!(&peer, r#"{{"ev":"spawned","id":"s2"}}"#).unwrap();
        let m = block_on(rx.next()).unwrap();
        assert_eq!((m.ev.as_str(), m.id.as_str()), ("spawned", "s2"));

        drop(peer);
        let m = block_on(rx.next()).unwrap();
        assert_eq!((m.ev.as_str(), m.error.as_str()), ("error", "pocketd disconnected"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
