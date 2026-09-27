use base64::{Engine, engine::general_purpose::STANDARD};
use futures::channel::mpsc::{UnboundedReceiver, unbounded};
use serde::Deserialize;
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Info {
    pub id: String,
    pub cmd: String,
    pub args: Vec<String>,
    pub cwd: String,
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

/// Builds a spawn op for a command line typed in the new-session form. The
/// desktop's own env goes along so pocketd resolves the command on this PATH.
pub fn spawn_op(cmdline: &str, cwd: &str) -> Option<Value> {
    let mut words = cmdline.split_whitespace();
    let cmd = words.next()?;
    let env: Vec<String> = std::env::vars().map(|(k, v)| format!("{k}={v}")).collect();
    let cwd = resolve_cwd(cwd, &std::env::var("HOME").unwrap_or_default());
    Some(json!({"op": "spawn", "cmd": cmd, "args": words.collect::<Vec<_>>(), "cwd": cwd, "env": env, "cols": 120, "rows": 36}))
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
        let m: Msg = serde_json::from_str(r#"{"ev":"sessions","items":[{"id":"a","cmd":"claude","args":["-c"],"cwd":"/w","cols":80,"rows":24}]}"#).unwrap();
        assert_eq!(m.items, vec![Info { id: "a".into(), cmd: "claude".into(), args: vec!["-c".into()], cwd: "/w".into() }]);
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
    fn spawn_op_splits_the_command_line() {
        let op = spawn_op("  codex -s read-only ", "/w").unwrap();
        assert_eq!(op["cmd"], "codex");
        assert_eq!(op["args"], json!(["-s", "read-only"]));
        assert_eq!(op["cwd"], "/w");
        assert!(op["env"].as_array().unwrap().iter().any(|e| e.as_str().unwrap().starts_with("PATH=")));
        assert_eq!(spawn_op("   ", "/w"), None);
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
