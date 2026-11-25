//! Runs commands and file operations on a host through its pocketd, over the tunnel `link` keeps up.

use crate::split;
use base64::{Engine, engine::general_purpose::STANDARD};
use daemon::Endpoint;
use daemon::UnixStream;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{self, BufRead, BufReader, ErrorKind, Write};
use std::net::Shutdown;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

/// The caps a host's pocketd needs for its projects to open here.
pub const NEEDS: [&str; 2] = ["exec.v1", "fs.v1"];

/// How long a file operation may take.
const FS_TIMEOUT: Duration = Duration::from_secs(30);
/// Added to an exec's own timeout, which pocketd enforces, for the reply's trip back.
const REPLY_SLACK: Duration = Duration::from_secs(5);

/// What a host's `status` says about the machine.
#[derive(Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct HostInfo {
    pub host_id: String,
    /// pocketd's own folder on the host, which may still be the legacy one there.
    pub home: String,
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

impl Conn {
    /// The reader thread holds a clone of the socket, which stays open until it's shut.
    fn close(&self) {
        let _ = self.stream.shutdown(Shutdown::Both);
    }

    fn idle(&self) -> bool {
        self.waiters.lock().expect("poisoned").as_ref().is_none_or(|w| w.is_empty())
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        self.conn.get_mut().expect("poisoned").iter().chain(self.retired.get_mut().expect("poisoned").iter()).for_each(Conn::close);
    }
}

/// A host's pocketd, reached through the socket its tunnel forwards. Paths may be the host's own or desktop keys under this host.
pub struct Client {
    id: String,
    endpoint: Endpoint,
    conn: Mutex<Option<Conn>>,
    next: AtomicU64,
    /// An exec's cwd when it names none; `/` isn't absolute on Windows, so there it's the home `status` gave.
    root: OnceLock<String>,
    home: OnceLock<String>,
    /// pocketd's own folder, as the first `status` gave it.
    pocket: OnceLock<String>,
    /// Connections a failed write retired, left open until the calls already sent on them are answered or give up.
    retired: Mutex<Vec<Conn>>,
}

impl Client {
    pub fn new(id: &str, sock: PathBuf) -> Self {
        Self::to(id, Endpoint { path: sock, token: None })
    }

    pub fn to(id: &str, endpoint: Endpoint) -> Self {
        Self {
            id: id.into(),
            endpoint,
            conn: Mutex::new(None),
            next: AtomicU64::new(1),
            root: OnceLock::new(),
            home: OnceLock::new(),
            pocket: OnceLock::new(),
            retired: Mutex::default(),
        }
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
        let waiters = {
            let mut conn = self.conn.lock().expect("poisoned");
            if conn.as_ref().is_some_and(|c| c.waiters.lock().expect("poisoned").is_none()) {
                *conn = None;
            }
            let open = match conn.as_mut() {
                Some(c) => c,
                None => conn.insert(open(&self.endpoint, FS_TIMEOUT)?),
            };
            open.waiters.lock().expect("poisoned").get_or_insert_default().insert(id.clone(), tx);
            if let Err(e) = writeln!(open.stream, "{op}") {
                open.waiters.lock().expect("poisoned").as_mut().map(|w| w.remove(&id));
                self.retire(conn.take());
                return Err(e);
            }
            open.waiters.clone()
        };
        let reply = rx.recv_timeout(timeout).map_err(|e| match e {
            mpsc::RecvTimeoutError::Timeout => io::Error::new(ErrorKind::TimedOut, "host did not answer in time"),
            mpsc::RecvTimeoutError::Disconnected => io::Error::new(ErrorKind::ConnectionAborted, "lost the host"),
        });
        if reply.is_err() {
            waiters.lock().expect("poisoned").as_mut().map(|w| w.remove(&id));
        }
        self.retire(None);
        let reply = reply?;
        if reply["ev"] == "error" {
            return Err(io::Error::other(reply["error"].as_str().unwrap_or("host refused").to_string()));
        }
        Ok(reply)
    }

    /// Adds `conn` to the retired ones and closes those with no call left waiting.
    fn retire(&self, conn: Option<Conn>) {
        let mut retired = self.retired.lock().expect("poisoned");
        retired.extend(conn);
        retired.retain(|c| {
            let idle = c.idle();
            if idle {
                c.close();
            }
            !idle
        });
    }

    fn fs(&self, op: &str, fields: Value) -> io::Result<Value> {
        let mut msg = fields;
        msg["op"] = op.into();
        self.call(msg, FS_TIMEOUT)
    }

    /// pocketd answers `status` without echoing an id, so it gets a connection of its own.
    pub fn status(&self) -> io::Result<HostInfo> {
        let mut stream = self.endpoint.connect(FS_TIMEOUT).map_err(io::Error::other)?;
        stream.set_read_timeout(Some(FS_TIMEOUT))?;
        writeln!(stream, "{}", json!({"op": "status"}))?;
        for line in BufReader::new(stream).lines() {
            let reply: Value = serde_json::from_str(&line?).unwrap_or_default();
            match reply["ev"].as_str() {
                Some("status") => {
                    let info: HostInfo = serde_json::from_value(reply["status"].clone()).map_err(io::Error::other)?;
                    let home = info.user_home.replace('\\', "/");
                    if !home.is_empty() {
                        if info.os == "windows" {
                            let _ = self.root.set(home.clone());
                        }
                        let _ = self.home.set(home);
                    }
                    let _ = self.pocket.set(pocket_dir(&info));
                    return Ok(info);
                }
                Some("error") => return Err(io::Error::other(reply["error"].as_str().unwrap_or("host refused").to_string())),
                _ => {}
            }
        }
        Err(io::Error::new(ErrorKind::ConnectionAborted, "lost the host"))
    }

    /// The host's home folder with `/` separators, asked of pocketd only until a `status` gives it.
    pub fn home(&self) -> io::Result<String> {
        match self.home.get() {
            Some(home) => Ok(home.clone()),
            None => Ok(self.status()?.user_home.replace('\\', "/")),
        }
    }

    /// pocketd's own folder on the host with `/` separators, once a `status` gave it.
    pub fn pocket_dir(&self) -> Option<String> {
        self.pocket.get().cloned()
    }

    pub fn list(&self, dir: &str) -> io::Result<Vec<Entry>> {
        let reply = self.fs("fs.list", json!({"path": self.host_path(dir)}))?;
        // pocketd omits `entries` for an empty folder.
        match &reply["entries"] {
            Value::Null => Ok(Vec::new()),
            entries => Vec::deserialize(entries).map_err(|e| io::Error::new(ErrorKind::InvalidData, e)),
        }
    }

    pub fn write(&self, path: &str, data: &[u8], create: bool) -> io::Result<()> {
        self.fs("fs.write", json!({"path": self.host_path(path), "data": STANDARD.encode(data), "create": create})).map(drop)
    }

    /// Adds what zstd `frame` decodes to at the end of `path`; needs `fs.append.v1`.
    pub fn append_frame(&self, path: &str, frame: &[u8]) -> io::Result<()> {
        self.fs("fs.append", json!({"path": self.host_path(path), "data": STANDARD.encode(frame)})).map(drop)
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
}

/// pocketd's own folder: the one `status` names, else where this channel keeps it under the user's home.
fn pocket_dir(info: &HostInfo) -> String {
    match info.home.as_str() {
        "" => format!("{}/{}", info.user_home, channel::home_dir_name()),
        home => home.replace('\\', "/"),
    }
}

/// A write that blocks past `write_timeout` fails, so a host that stopped reading can't hold the lock `call` writes under.
fn open(endpoint: &Endpoint, write_timeout: Duration) -> io::Result<Conn> {
    let stream = endpoint.connect(FS_TIMEOUT).map_err(io::Error::other)?;
    stream.set_write_timeout(Some(write_timeout))?;
    let waiters: Waiters = Arc::new(Mutex::new(Some(HashMap::new())));
    let reader = BufReader::new(stream.try_clone()?);
    let shared = waiters.clone();
    std::thread::spawn(move || {
        for line in reader.lines().map_while(Result::ok) {
            let Ok(reply) = serde_json::from_str::<Value>(&line) else { continue };
            let Some(id) = reply["id"].as_str() else { continue };
            if let Some(tx) = shared.lock().expect("poisoned").as_mut().and_then(|w| w.remove(id)) {
                let _ = tx.send(reply);
            }
        }
        // Dropping the senders fails every call still waiting.
        shared.lock().expect("poisoned").take();
    });
    Ok(Conn { stream, waiters })
}

fn bytes(v: &Value) -> io::Result<Vec<u8>> {
    v.as_str().map_or(Ok(Vec::new()), |s| STANDARD.decode(s).map_err(|e| io::Error::new(ErrorKind::InvalidData, e)))
}

impl git::Exec for Client {
    fn exec(&self, cwd: &str, argv: &[&str], stdin: Option<&[u8]>, timeout: Duration) -> io::Result<git::Output> {
        let argv: Vec<&str> = argv.iter().map(|a| self.host_path(a)).collect();
        // pocketd wants an absolute cwd; git's `-C` names the real one.
        let cwd = if cwd.is_empty() { self.root.get().map_or("/", String::as_str) } else { self.host_path(cwd) };
        let mut op = json!({"op": "exec", "cwd": cwd, "argv": argv, "timeoutMs": timeout.as_millis() as u64});
        if let Some(input) = stdin {
            op["stdin"] = STANDARD.encode(input).into();
        }
        let reply = self.call(op, timeout + REPLY_SLACK)?;
        // pocketd omits a zero `code`.
        Ok(git::Output { code: reply["code"].as_i64().unwrap_or(0) as i32, stdout: bytes(&reply["stdout"])?, stderr: bytes(&reply["stderr"])? })
    }

    fn read(&self, path: &str, max: usize) -> io::Result<Vec<u8>> {
        let reply = self.fs("fs.read", json!({"path": self.host_path(path), "max": max}))?;
        if reply["truncated"] == true {
            return Err(git::too_large(max));
        }
        bytes(&reply["data"])
    }

    fn stat(&self, path: &str) -> io::Result<Option<git::Stat>> {
        let reply = self.fs("fs.stat", json!({"path": self.host_path(path)}))?;
        if reply["exists"] != true {
            return Ok(None);
        }
        // pocketd's stat has no creation time, and its mtime would order worktrees by their last edit.
        let modified = reply["mtime"].as_u64().map(|ms| std::time::UNIX_EPOCH + Duration::from_millis(ms));
        Ok(Some(git::Stat { dir: reply["dir"] == true, size: reply["size"].as_u64().unwrap_or(0), created: None, modified }))
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
    registry().lock().expect("poisoned").get(id).cloned()
}

pub fn register(client: Arc<Client>) {
    registry().lock().expect("poisoned").insert(client.id.clone(), client);
}

pub fn forget(id: &str) {
    registry().lock().expect("poisoned").remove(id);
}

#[cfg(test)]
mod tests {
    use super::*;
    use git::Exec;
    #[cfg(unix)]
    use std::os::unix::net::UnixListener;
    #[cfg(windows)]
    use uds_windows::UnixListener;

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
                        Some("exec") => {
                            json!({"ev": "exec", "id": msg["id"], "code": 0, "stdout": STANDARD.encode(format!("{}|{}", msg["argv"][1].as_str().unwrap(), msg["cwd"].as_str().unwrap()))})
                        }
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
    fn pockets_own_folder_is_the_one_status_names_else_this_channels_under_the_users_home() {
        let info = |home: &str, user_home: &str| HostInfo { home: home.into(), user_home: user_home.into(), ..HostInfo::default() };
        assert_eq!(pocket_dir(&info("", "/home/dev")), format!("/home/dev/{}", channel::home_dir_name()));
        assert_eq!(pocket_dir(&info("C:\\Users\\dev\\.coding-pocket", "C:/Users/dev")), "C:/Users/dev/.coding-pocket");
    }

    #[test]
    fn home_is_remembered_from_the_first_status() {
        let sock = temp_sock("home");
        serve(&sock);
        let client = Client::new("devbox", sock.clone());
        assert_eq!(client.home().unwrap(), "/home/dev");
        std::fs::remove_file(&sock).unwrap();
        assert_eq!(client.home().unwrap(), "/home/dev");
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
    fn a_write_the_host_never_drains_fails_instead_of_holding_the_client() {
        let sock = temp_sock("undrained");
        let listener = UnixListener::bind(&sock).unwrap();
        std::thread::spawn(move || drop(listener.incoming().flatten().collect::<Vec<_>>()));
        let client = Client::new("devbox", sock.clone());
        *client.conn.lock().unwrap() = Some(open(&client.endpoint, Duration::from_millis(300)).unwrap());
        let (tx, done) = mpsc::channel();
        std::thread::spawn(move || tx.send(client.exec("/", &["cat"], Some(&vec![b'x'; 8 << 20]), Duration::from_millis(100))));
        let reply = done.recv_timeout(Duration::from_secs(2)).expect("the write blocked past the call's own timeout");
        assert!(reply.is_err());
        let _ = std::fs::remove_file(sock);
    }

    /// A pocketd stand-in that answers each call with `reply`'s result for it.
    fn serve_with(sock: &PathBuf, reply: impl Fn(&Value) -> Value + Send + 'static) {
        let listener = UnixListener::bind(sock).unwrap();
        std::thread::spawn(move || {
            let Ok((stream, _)) = listener.accept() else { return };
            let mut writer = stream.try_clone().unwrap();
            for line in BufReader::new(stream).lines().map_while(Result::ok) {
                let msg: Value = serde_json::from_str(&line).unwrap();
                writeln!(writer, "{}", reply(&msg)).unwrap();
            }
        });
    }

    #[test]
    fn a_folder_reply_of_the_wrong_shape_is_invalid_data_but_one_without_entries_is_an_empty_folder() {
        let sock = temp_sock("entries");
        serve_with(&sock, |m| match m["path"].as_str() {
            Some("/bad") => json!({"ev": "fs.list", "id": m["id"], "entries": [{"name": "a", "size": -1}]}),
            Some("/null") => json!({"ev": "fs.list", "id": m["id"], "entries": null}),
            Some("/some") => json!({"ev": "fs.list", "id": m["id"], "entries": [{"name": "a", "dir": true}]}),
            _ => json!({"ev": "fs.list", "id": m["id"]}),
        });
        let client = Client::new("devbox", sock.clone());
        assert_eq!(client.list("/bad").unwrap_err().kind(), ErrorKind::InvalidData);
        assert_eq!(client.list("/null").unwrap(), vec![]);
        assert_eq!(client.list("/omitted").unwrap(), vec![]);
        assert_eq!(client.list("/some").unwrap(), vec![Entry { name: "a".into(), dir: true, ..Entry::default() }]);
        let _ = std::fs::remove_file(sock);
    }

    #[test]
    fn output_that_is_not_base64_is_invalid_data_but_omitted_output_is_empty() {
        let sock = temp_sock("base64");
        serve_with(&sock, |m| match m["argv"][0].as_str() {
            Some("garbled") => json!({"ev": "exec", "id": m["id"], "stdout": "!!not base64!!"}),
            _ => json!({"ev": "exec", "id": m["id"]}),
        });
        let client = Client::new("devbox", sock.clone());
        let garbled = client.exec("/", &["garbled"], None, Duration::from_secs(5));
        assert_eq!(garbled.unwrap_err().kind(), ErrorKind::InvalidData);
        let silent = client.exec("/", &["silent"], None, Duration::from_secs(5)).unwrap();
        assert_eq!((silent.code, silent.stdout, silent.stderr), (0, vec![], vec![]));
        let _ = std::fs::remove_file(sock);
    }

    #[test]
    fn a_write_that_times_out_leaves_the_calls_in_flight_their_replies_then_closes() {
        let sock = temp_sock("inflight");
        let listener = UnixListener::bind(&sock).unwrap();
        let (closed_tx, closed) = mpsc::channel();
        std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut writer = stream.try_clone().unwrap();
            let mut first = String::new();
            BufReader::new(&stream).read_line(&mut first).unwrap();
            let msg: Value = serde_json::from_str(&first).unwrap();
            std::thread::sleep(Duration::from_millis(1000));
            writeln!(writer, "{}", json!({"ev": "exec", "id": msg["id"]})).unwrap();
            let _ = std::io::copy(&mut &stream, &mut std::io::sink());
            let _ = closed_tx.send(());
        });
        let client = Arc::new(Client::new("devbox", sock.clone()));
        *client.conn.lock().unwrap() = Some(open(&client.endpoint, Duration::from_millis(300)).unwrap());
        let slow = {
            let client = client.clone();
            std::thread::spawn(move || client.exec("/", &["sleep"], None, Duration::from_secs(5)))
        };
        std::thread::sleep(Duration::from_millis(100));
        assert!(client.exec("/", &["cat"], Some(&vec![b'x'; 16 << 20]), Duration::from_secs(5)).is_err());
        assert!(slow.join().unwrap().is_ok(), "the slow call lost its reply");
        assert!(closed.recv_timeout(Duration::from_secs(2)).is_ok(), "the dropped connection was never closed");
        let _ = std::fs::remove_file(sock);
    }

    #[test]
    fn dropping_the_client_closes_its_connection() {
        let sock = temp_sock("dropped");
        let listener = UnixListener::bind(&sock).unwrap();
        let (tx, closed) = mpsc::channel();
        std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut writer = stream.try_clone().unwrap();
            for line in BufReader::new(stream).lines().map_while(Result::ok) {
                let msg: Value = serde_json::from_str(&line).unwrap();
                writeln!(writer, "{}", json!({"ev": "exec", "id": msg["id"]})).unwrap();
            }
            let _ = tx.send(());
        });
        let client = Client::new("devbox", sock.clone());
        client.exec("/", &["echo", "0"], None, Duration::from_secs(5)).unwrap();
        drop(client);
        assert!(closed.recv_timeout(Duration::from_secs(2)).is_ok(), "pocketd never saw the connection close");
        let _ = std::fs::remove_file(sock);
    }

    #[test]
    fn on_windows_a_command_without_a_cwd_runs_in_the_home_status_gave() {
        let sock = temp_sock("windows");
        let listener = UnixListener::bind(&sock).unwrap();
        std::thread::spawn(move || {
            for stream in listener.incoming().map_while(Result::ok) {
                let mut line = String::new();
                BufReader::new(&stream).read_line(&mut line).unwrap();
                let msg: Value = serde_json::from_str(&line).unwrap();
                let reply = match msg["op"].as_str() {
                    Some("status") => json!({"ev": "status", "status": {"os": "windows", "userHome": r"C:\Users\me"}}),
                    _ => json!({"ev": "exec", "id": msg["id"], "code": 0, "stdout": STANDARD.encode(msg["cwd"].as_str().unwrap())}),
                };
                writeln!(&stream, "{reply}").unwrap();
            }
        });
        let client = Client::new("pc", sock.clone());
        client.status().unwrap();
        let out = client.exec("", &["git", "--version"], None, Duration::from_secs(5)).unwrap();
        assert_eq!(String::from_utf8(out.stdout).unwrap(), "C:/Users/me");
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
}
