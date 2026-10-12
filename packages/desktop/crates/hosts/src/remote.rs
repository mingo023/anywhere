//! Runs commands and file operations on a host through its pocketd, over the tunnel `link` keeps up.

use crate::split;
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{self, BufRead, BufReader, ErrorKind, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, UNIX_EPOCH};

/// The caps a host's pocketd needs for its projects to open here.
pub const NEEDS: [&str; 2] = ["exec.v1", "fs.v1"];

/// How long a file operation may take.
const FS_TIMEOUT: Duration = Duration::from_secs(30);
/// The most of a host's `desktop.json` read before adding projects to it.
const REGISTRY_MAX: usize = 16 << 20;
/// Added to an exec's own timeout, which pocketd enforces, for the reply's trip back.
const REPLY_SLACK: Duration = Duration::from_secs(5);

/// What a host's `status` says about the machine.
#[derive(Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct HostInfo {
    pub host_id: String,
    pub os: String,
    pub arch: String,
    pub user_home: String,
    pub shell: String,
    pub name: String,
    pub caps: Vec<String>,
}

impl HostInfo {
    /// Whether this pocketd can hold projects: older ones only run terminals.
    pub fn serves_projects(&self) -> bool {
        NEEDS.iter().all(|c| self.caps.iter().any(|have| have == c))
    }
}

/// One entry of a folder on the host.
#[derive(Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Entry {
    pub name: String,
    pub dir: bool,
    pub link: bool,
    pub size: u64,
    /// Unix ms.
    pub mtime: i64,
    pub ignored: bool,
}

/// The calls awaiting replies; `None` once the connection closed.
type Waiters = Arc<Mutex<Option<HashMap<String, Sender<Value>>>>>;

/// One connection, shared by every call; a reader thread hands each reply to the call with its id.
struct Conn {
    stream: UnixStream,
    waiters: Waiters,
}

/// A host's pocketd, reached through the socket its tunnel forwards. Paths may be the host's own or desktop keys under this host.
pub struct Client {
    id: String,
    sock: PathBuf,
    conn: Mutex<Option<Conn>>,
    next: AtomicU64,
}

impl Client {
    pub fn new(id: &str, sock: PathBuf) -> Self {
        Self { id: id.into(), sock, conn: Mutex::new(None), next: AtomicU64::new(1) }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    /// The host's path for `p`: a key under this host loses its `//<id>` prefix, anything else is already the host's.
    pub fn host_path<'a>(&self, p: &'a str) -> &'a str {
        match split(p) {
            (Some(id), path) if id == self.id => path,
            _ => p,
        }
    }

    /// The desktop's key for the host's `path`.
    pub fn key(&self, path: &str) -> String {
        crate::place(&self.id, path)
    }

    /// Sends `op` with a fresh id and waits up to `timeout` for the reply carrying it.
    fn call(&self, mut op: Value, timeout: Duration) -> io::Result<Value> {
        let id = self.next.fetch_add(1, Ordering::Relaxed).to_string();
        op["id"] = id.clone().into();
        let (tx, rx) = mpsc::channel();
        {
            let mut conn = self.conn.lock().unwrap();
            if conn.as_ref().is_some_and(|c| c.waiters.lock().unwrap().is_none()) {
                *conn = None;
            }
            let open = match conn.as_mut() {
                Some(c) => c,
                None => conn.insert(open(&self.sock)?),
            };
            open.waiters.lock().unwrap().get_or_insert_default().insert(id.clone(), tx);
            if let Err(e) = writeln!(open.stream, "{op}") {
                *conn = None;
                return Err(e);
            }
        }
        let reply = rx.recv_timeout(timeout).map_err(|e| match e {
            mpsc::RecvTimeoutError::Timeout => io::Error::new(ErrorKind::TimedOut, "host did not answer in time"),
            mpsc::RecvTimeoutError::Disconnected => io::Error::new(ErrorKind::ConnectionAborted, "lost the host"),
        });
        if reply.is_err()
            && let Some(c) = self.conn.lock().unwrap().as_ref()
        {
            c.waiters.lock().unwrap().as_mut().map(|w| w.remove(&id));
        }
        let reply = reply?;
        if reply["ev"] == "error" {
            return Err(io::Error::other(reply["error"].as_str().unwrap_or("host refused").to_string()));
        }
        Ok(reply)
    }

    fn fs(&self, op: &str, fields: Value) -> io::Result<Value> {
        let mut msg = fields;
        msg["op"] = op.into();
        self.call(msg, FS_TIMEOUT)
    }

    /// pocketd answers `status` without echoing an id, so it gets a connection of its own.
    pub fn status(&self) -> io::Result<HostInfo> {
        let mut stream = UnixStream::connect(&self.sock)?;
        stream.set_read_timeout(Some(FS_TIMEOUT))?;
        writeln!(stream, "{}", json!({"op": "status"}))?;
        for line in BufReader::new(stream).lines() {
            let reply: Value = serde_json::from_str(&line?).unwrap_or_default();
            match reply["ev"].as_str() {
                Some("status") => return serde_json::from_value(reply["status"].clone()).map_err(io::Error::other),
                Some("error") => return Err(io::Error::other(reply["error"].as_str().unwrap_or("host refused").to_string())),
                _ => {}
            }
        }
        Err(io::Error::new(ErrorKind::ConnectionAborted, "lost the host"))
    }

    pub fn list(&self, dir: &str) -> io::Result<Vec<Entry>> {
        let reply = self.fs("fs.list", json!({"path": self.host_path(dir)}))?;
        serde_json::from_value(reply["entries"].clone()).or_else(|_| Ok(Vec::new()))
    }

    pub fn write(&self, path: &str, data: &[u8], create: bool) -> io::Result<()> {
        self.fs("fs.write", json!({"path": self.host_path(path), "data": STANDARD.encode(data), "create": create})).map(drop)
    }

    pub fn mkdir(&self, path: &str, parents: bool) -> io::Result<()> {
        self.fs("fs.mkdir", json!({"path": self.host_path(path), "parents": parents})).map(drop)
    }

    pub fn rename(&self, from: &str, to: &str) -> io::Result<()> {
        self.fs("fs.rename", json!({"from": self.host_path(from), "to": self.host_path(to)})).map(drop)
    }

    pub fn remove(&self, path: &str, recursive: bool) -> io::Result<()> {
        self.fs("fs.remove", json!({"path": self.host_path(path), "recursive": recursive})).map(drop)
    }

    /// Moves `path` to the host's trash folder; the key of where it went.
    pub fn trash(&self, path: &str) -> io::Result<String> {
        let reply = self.fs("fs.trash", json!({"path": self.host_path(path)}))?;
        Ok(self.key(reply["trashed"].as_str().unwrap_or_default()))
    }

    /// Lists `ours`' projects in the `desktop.json` beside the host's pocketd, which makes worktrees only for those listed there.
    pub fn sync_registry(&self, ours: &Value) -> io::Result<()> {
        use git::Exec;
        let path = format!("{}/{}/desktop.json", self.status()?.user_home, channel::home_dir_name());
        let theirs = match self.stat(&path)? {
            Some(_) => serde_json::from_slice(&self.read(&path, REGISTRY_MAX)?).map_err(io::Error::other)?,
            None => json!({}),
        };
        let merged = merged(theirs.clone(), ours);
        if merged != theirs {
            self.write(&path, &serde_json::to_vec_pretty(&merged)?, false)?;
        }
        Ok(())
    }
}

/// `ours` laid over a host's `desktop.json`. A Mac host's own desktop keeps its store there, so the rest stays.
fn merged(mut theirs: Value, ours: &Value) -> Value {
    if !theirs.is_object() {
        theirs = json!({});
    }
    let mut projects = theirs["projects"].as_array().cloned().unwrap_or_default();
    for p in ours["projects"].as_array().into_iter().flatten() {
        if !projects.contains(p) {
            projects.push(p.clone());
        }
    }
    theirs["projects"] = projects.into();
    for (path, repo) in ours["repos"].as_object().into_iter().flatten() {
        if !theirs["repos"][path].is_object() {
            theirs["repos"][path] = json!({});
        }
        for (key, value) in repo.as_object().into_iter().flatten() {
            theirs["repos"][path][key] = value.clone();
        }
    }
    theirs
}

fn open(sock: &PathBuf) -> io::Result<Conn> {
    let stream = UnixStream::connect(sock)?;
    let waiters: Waiters = Arc::new(Mutex::new(Some(HashMap::new())));
    let reader = BufReader::new(stream.try_clone()?);
    let shared = waiters.clone();
    std::thread::spawn(move || {
        for line in reader.lines().map_while(Result::ok) {
            let Ok(reply) = serde_json::from_str::<Value>(&line) else { continue };
            let Some(id) = reply["id"].as_str() else { continue };
            if let Some(tx) = shared.lock().unwrap().as_mut().and_then(|w| w.remove(id)) {
                let _ = tx.send(reply);
            }
        }
        // Dropping the senders fails every call still waiting.
        shared.lock().unwrap().take();
    });
    Ok(Conn { stream, waiters })
}

fn bytes(v: &Value) -> Vec<u8> {
    v.as_str().and_then(|s| STANDARD.decode(s).ok()).unwrap_or_default()
}

impl git::Exec for Client {
    fn exec(&self, cwd: &str, argv: &[&str], stdin: Option<&[u8]>, timeout: Duration) -> io::Result<git::Output> {
        let argv: Vec<&str> = argv.iter().map(|a| self.host_path(a)).collect();
        // pocketd wants an absolute cwd; git's `-C` names the real one.
        let cwd = if cwd.is_empty() { "/" } else { self.host_path(cwd) };
        let mut op = json!({"op": "exec", "cwd": cwd, "argv": argv, "timeoutMs": timeout.as_millis() as u64});
        if let Some(input) = stdin {
            op["stdin"] = STANDARD.encode(input).into();
        }
        let reply = self.call(op, timeout + REPLY_SLACK)?;
        Ok(git::Output { code: reply["code"].as_i64().unwrap_or(0) as i32, stdout: bytes(&reply["stdout"]), stderr: bytes(&reply["stderr"]) })
    }

    fn read(&self, path: &str, max: usize) -> io::Result<Vec<u8>> {
        let reply = self.fs("fs.read", json!({"path": self.host_path(path), "max": max.min(i64::MAX as usize)}))?;
        Ok(bytes(&reply["data"]))
    }

    fn stat(&self, path: &str) -> io::Result<Option<git::Stat>> {
        let reply = self.fs("fs.stat", json!({"path": self.host_path(path)}))?;
        if reply["exists"] != true {
            return Ok(None);
        }
        let mtime = reply["mtime"].as_u64().map(|ms| UNIX_EPOCH + Duration::from_millis(ms));
        Ok(Some(git::Stat { dir: reply["dir"] == true, size: reply["size"].as_u64().unwrap_or(0), created: mtime }))
    }

    fn place(&self, path: &str) -> String {
        self.key(path)
    }
}

fn registry() -> &'static Mutex<HashMap<String, Arc<Client>>> {
    static HOSTS: OnceLock<Mutex<HashMap<String, Arc<Client>>>> = OnceLock::new();
    HOSTS.get_or_init(Mutex::default)
}

/// The client for host `id` while its link is up and its pocketd serves projects.
pub fn connected(id: &str) -> Option<Arc<Client>> {
    registry().lock().unwrap().get(id).cloned()
}

pub fn register(client: Arc<Client>) {
    registry().lock().unwrap().insert(client.id.clone(), client);
}

pub fn forget(id: &str) {
    registry().lock().unwrap().remove(id);
}

#[cfg(test)]
mod tests {
    use super::*;
    use git::Exec;
    use std::os::unix::net::UnixListener;

    fn temp_sock(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("hr-{name}-{}.sock", std::process::id()));
        let _ = std::fs::remove_file(&path);
        path
    }

    /// A pocketd stand-in that answers each exec after `delay_ms` from its argv, so later calls can be answered first.
    fn serve(sock: &PathBuf) {
        let listener = UnixListener::bind(sock).unwrap();
        std::thread::spawn(move || {
            let Ok((stream, _)) = listener.accept() else { return };
            let writer = Arc::new(Mutex::new(stream.try_clone().unwrap()));
            for line in BufReader::new(stream).lines().map_while(Result::ok) {
                let msg: Value = serde_json::from_str(&line).unwrap();
                let writer = writer.clone();
                std::thread::spawn(move || {
                    let delay: u64 = msg["argv"][1].as_str().unwrap_or("0").parse().unwrap_or(0);
                    std::thread::sleep(Duration::from_millis(delay));
                    let reply = match msg["op"].as_str() {
                        Some("exec") if msg["argv"][0] == "fail" => json!({"ev": "error", "id": msg["id"], "error": "no such command"}),
                        Some("exec") => json!({"ev": "exec", "id": msg["id"], "code": 0, "stdout": STANDARD.encode(format!("{}|{}", msg["argv"][1].as_str().unwrap(), msg["cwd"].as_str().unwrap()))}),
                        // pocketd answers status without the id.
                        Some("status") => json!({"ev": "status", "status": {"userHome": "/home/dev", "caps": ["exec.v1", "fs.v1"]}}),
                        _ => json!({"ev": "fs.stat", "id": msg["id"]}),
                    };
                    writeln!(writer.lock().unwrap(), "{reply}").unwrap();
                });
            }
        });
    }

    #[test]
    fn status_is_read_though_pocketd_answers_it_without_the_id() {
        let sock = temp_sock("status");
        serve(&sock);
        let info = Client::new("devbox", sock.clone()).status().unwrap();
        assert_eq!(info.user_home, "/home/dev");
        assert!(info.serves_projects());
        let _ = std::fs::remove_file(sock);
    }

    #[test]
    fn concurrent_calls_each_get_their_own_reply() {
        let sock = temp_sock("concurrent");
        serve(&sock);
        let client = Arc::new(Client::new("devbox", sock.clone()));
        let calls: Vec<_> = [120, 10, 60, 0]
            .into_iter()
            .map(|delay| {
                let client = client.clone();
                std::thread::spawn(move || {
                    let out = client.exec("/srv", &["echo", &delay.to_string()], None, Duration::from_secs(5)).unwrap();
                    (delay, String::from_utf8(out.stdout).unwrap())
                })
            })
            .collect();
        for call in calls {
            let (delay, out) = call.join().unwrap();
            assert_eq!(out, format!("{delay}|/srv"));
        }
        let _ = std::fs::remove_file(sock);
    }

    #[test]
    fn a_key_under_the_host_reaches_it_as_the_hosts_own_path() {
        let sock = temp_sock("keys");
        serve(&sock);
        let client = Client::new("devbox", sock.clone());
        let out = client.exec("//devbox/home/me/app", &["echo", "0"], None, Duration::from_secs(5)).unwrap();
        assert_eq!(String::from_utf8(out.stdout).unwrap(), "0|/home/me/app");
        assert_eq!(client.host_path("//other/x"), "//other/x");
        assert_eq!(client.key("/home/me/app"), "//devbox/home/me/app");
        let _ = std::fs::remove_file(sock);
    }

    #[test]
    fn a_refusal_comes_back_as_an_error_and_missing_files_as_none() {
        let sock = temp_sock("refusal");
        serve(&sock);
        let client = Client::new("devbox", sock.clone());
        let err = client.exec("/", &["fail", "0"], None, Duration::from_secs(5)).unwrap_err();
        assert_eq!(err.to_string(), "no such command");
        assert_eq!(client.stat("/nope").unwrap(), None);
        let _ = std::fs::remove_file(sock);
    }

    #[test]
    fn a_host_that_never_answers_times_out() {
        let sock = temp_sock("silent");
        serve(&sock);
        let client = Client::new("devbox", sock.clone());
        let err = client.call(json!({"op": "exec", "cwd": "/", "argv": ["echo", "2000"]}), Duration::from_millis(50)).unwrap_err();
        assert!(matches!(err.kind(), ErrorKind::TimedOut), "{err}");
        let _ = std::fs::remove_file(sock);
    }

    #[test]
    fn a_host_without_a_tunnel_fails_at_once() {
        let client = Client::new("devbox", temp_sock("absent"));
        assert!(client.status().is_err());
    }

    #[test]
    fn only_a_pocketd_with_exec_and_fs_serves_projects() {
        let info = |caps: &[&str]| HostInfo { caps: caps.iter().map(|c| c.to_string()).collect(), ..HostInfo::default() };
        assert!(info(&["host.v2", "fs.v1", "exec.v1"]).serves_projects());
        assert!(!info(&["fs.v1"]).serves_projects());
    }

    #[test]
    fn our_projects_join_a_hosts_desktop_json_without_losing_what_its_own_desktop_keeps() {
        let theirs = json!({"projects": ["/u/site"], "repos": {"/u/app": {"color": 3, "setup": "make"}}, "sounds": {"done": false}});
        let ours = json!({"projects": ["/u/app"], "repos": {"/u/app": {"name": "app", "setup": "npm i"}}});
        let got = merged(theirs, &ours);
        assert_eq!(got["projects"], json!(["/u/site", "/u/app"]));
        assert_eq!(got["repos"]["/u/app"], json!({"color": 3, "setup": "npm i", "name": "app"}));
        assert_eq!(got["sounds"], json!({"done": false}));
        assert_eq!(merged(got.clone(), &ours), got, "a second sync changes nothing");
        assert_eq!(merged(Value::Null, &ours)["projects"], json!(["/u/app"]));
    }
}
