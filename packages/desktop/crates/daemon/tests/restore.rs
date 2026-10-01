//! Restarts a scratch pocketd under the desktop's own client and checks that every Terminal comes back by id.

use daemon::{Daemon, Msg};
use futures::channel::mpsc::{TryRecvError, UnboundedReceiver};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

struct Pocketd {
    bin: PathBuf,
    home: PathBuf,
    child: Option<Child>,
}

impl Pocketd {
    fn build(home: &Path) -> Self {
        let bin = home.join("pocketd");
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../pocketd");
        let built = Command::new("go").args(["build", "-o"]).arg(&bin).arg("./cmd/pocketd").current_dir(src).status().unwrap();
        assert!(built.success(), "go build failed");
        Self { bin, home: home.into(), child: None }
    }

    fn sock(&self) -> PathBuf {
        self.home.join("d.sock")
    }

    /**
     * resumeAgents is off: a restored Terminal runs in the login env, where the real claude is.
     * POCKETD_RESTORE_SHELL keeps the owner's login shell and rc files out of restored Terminals.
     */
    fn start(&mut self) {
        let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let config = json!({"token": "t", "port": port, "listen": "loopback", "restore": {"resumeAgents": false}});
        std::fs::write(self.home.join("config.json"), config.to_string()).unwrap();
        let child = Command::new(&self.bin)
            .arg("serve")
            .env("HOME", &self.home)
            .env("POCKET_HOME", &self.home)
            .env("POCKETD_SOCK", self.sock())
            .env("POCKETD_RESTORE_SHELL", "/bin/sh")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        self.child = Some(child);
    }

    fn stop(&mut self) {
        let mut child = self.child.take().unwrap();
        unsafe { libc::kill(child.id() as i32, libc::SIGTERM) };
        assert!(child.wait().unwrap().success(), "pocketd did not exit 0 on SIGTERM");
    }
}

impl Drop for Pocketd {
    fn drop(&mut self) {
        if let Some(child) = &mut self.child {
            let _ = child.kill();
            let _ = child.wait();
        }
        let _ = std::fs::remove_dir_all(&self.home);
    }
}

fn wait(rx: &mut UnboundedReceiver<Msg>, what: &str, ok: impl Fn(&Msg) -> bool) -> Msg {
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        match rx.try_recv() {
            Ok(m) if ok(&m) => return m,
            Ok(_) => {}
            Err(TryRecvError::Closed) => panic!("the client closed while waiting for {what}"),
            Err(TryRecvError::Empty) => std::thread::sleep(Duration::from_millis(20)),
        }
    }
    panic!("no {what} within 30 s");
}

#[test]
#[ignore = "builds and runs a scratch pocketd"]
fn terminals_come_back_by_id_after_a_restart() {
    let home = PathBuf::from(format!("/tmp/pk-restore-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    let mut pd = Pocketd::build(&home);
    pd.start();
    let (d, mut rx) = Daemon::spawn(&pd.sock());
    wait(&mut rx, "up", |m| m.ev == "up");
    let mut ids: Vec<String> = (0..2)
        .map(|_| {
            assert!(d.send(json!({"op": "spawn", "cmd": "/bin/sh", "args": ["-l"], "cwd": home, "cols": 80, "rows": 24})));
            wait(&mut rx, "spawned", |m| m.ev == "spawned").id
        })
        .collect();

    pd.stop();
    wait(&mut rx, "down", |m| m.ev == "down");
    pd.start();
    wait(&mut rx, "up", |m| m.ev == "up");

    assert!(d.send(json!({"op": "list"})));
    let mut listed: Vec<String> = wait(&mut rx, "terminals", |m| m.ev == "terminals").items.into_iter().map(|i| i.id).collect();
    listed.sort();
    ids.sort();
    assert_eq!(listed, ids);
    for id in &ids {
        assert!(d.send(json!({"op": "attach", "id": id})));
        wait(&mut rx, "a snapshot", |m| m.ev == "snapshot" && &m.id == id);
    }
}
