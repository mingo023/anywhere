//! A Windows host's pocketd as the desktop reaches it: its owner TCP port behind a local socket, as `ssh -L sock:127.0.0.1:port`
//! forwards it. Runs a scratch macOS pocketd with ownerListen on, which speaks the same auth as on Windows.

use daemon::{Daemon, Endpoint, NOT_OWNER, request_to};
use futures::StreamExt;
use futures::executor::block_on;
use git::Exec;
use hosts::remote::Client;
use serde_json::{Value, json};
use std::net::TcpStream;
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

struct Pocketd {
    home: PathBuf,
    child: Child,
}

impl Drop for Pocketd {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.home);
    }
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

/// Every pocketd run gets its own home and socket, or it would take over the one the developer's terminals use.
fn pocketd(home: &Path, bin: &Path, args: &[&str]) -> Command {
    let mut cmd = Command::new(bin);
    cmd.args(args).env("HOME", home).env("POCKET_HOME", home).env("POCKETD_SOCK", home.join("d.sock"));
    cmd
}

/// Starts pocketd with ownerListen on and mints a token the way `link` does: `pocketd pair --json --name`.
fn start(home: &Path) -> (Pocketd, u16, String) {
    std::fs::create_dir_all(home).unwrap();
    let bin = home.join("pocketd");
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../pocketd");
    assert!(Command::new("go").args(["build", "-o"]).arg(&bin).arg("./cmd/pocketd").current_dir(src).status().unwrap().success());
    let owner = free_port();
    let config = json!({"port": free_port(), "listen": "loopback", "ownerListen": format!("127.0.0.1:{owner}")});
    std::fs::write(home.join("config.json"), config.to_string()).unwrap();
    let pair = pocketd(home, &bin, &["pair", "--json", "--name", "Test Mac"]).output().unwrap();
    assert!(pair.status.success(), "{}", String::from_utf8_lossy(&pair.stderr));
    let pair: Value = serde_json::from_slice(&pair.stdout).unwrap();
    assert_eq!(pair["port"], owner);
    let child = pocketd(home, &bin, &["serve"]).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap();
    (Pocketd { home: home.into(), child }, owner, pair["token"].as_str().unwrap().into())
}

/// What ssh's `-L <sock>:127.0.0.1:<port>` does on this side: each connection to `sock` becomes one to `port`.
fn forward(sock: &Path, port: u16) {
    let listener = UnixListener::bind(sock).unwrap();
    std::thread::spawn(move || {
        for local in listener.incoming().map_while(Result::ok) {
            let Ok(remote) = TcpStream::connect(("127.0.0.1", port)) else { continue };
            let (mut a, mut b) = (local.try_clone().unwrap(), remote.try_clone().unwrap());
            std::thread::spawn(move || {
                let _ = std::io::copy(&mut a, &mut b);
                let _ = b.shutdown(std::net::Shutdown::Both);
            });
            let (mut a, mut b) = (remote, local);
            std::thread::spawn(move || {
                let _ = std::io::copy(&mut a, &mut b);
                let _ = b.shutdown(std::net::Shutdown::Both);
            });
        }
    });
}

#[test]
#[ignore = "builds and runs a scratch pocketd"]
fn every_desktop_client_is_owner_over_the_forwarded_tcp_port_with_a_paired_token() {
    let home = PathBuf::from(format!("/tmp/pk-owner-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let (_pd, port, token) = start(&home);
    let sock = home.join("fwd.sock");
    forward(&sock, port);
    let endpoint = Endpoint { path: sock.clone(), token: Some(token.clone()) };
    let status = json!({"op": "status"});

    let deadline = Instant::now() + Duration::from_secs(20);
    while request_to(&endpoint, &status, Duration::from_secs(2)).is_err() {
        assert!(Instant::now() < deadline, "pocketd never answered on its owner port");
        std::thread::sleep(Duration::from_millis(100));
    }
    assert_eq!(request_to(&endpoint, &status, Duration::from_secs(5)).unwrap().ev, "status");

    let stranger = request_to(&Endpoint::local(&sock), &status, Duration::from_secs(5)).unwrap_err();
    assert!(stranger.contains("needs observe"), "{stranger}");
    let forged = Endpoint { path: sock.clone(), token: Some("forged".into()) };
    assert_eq!(request_to(&forged, &status, Duration::from_secs(5)).unwrap_err(), NOT_OWNER);

    let (d, mut rx) = Daemon::spawn_to(endpoint.clone());
    assert_eq!(block_on(rx.next()).unwrap().ev, "up");
    assert!(d.send(json!({"op": "list"})));
    assert_eq!(block_on(rx.next()).unwrap().ev, "terminals");

    let client = Client::to("pc", endpoint.clone());
    assert!(client.status().unwrap().serves_projects());
    let out = client.exec("/", &["echo", "hi"], None, Duration::from_secs(10)).unwrap();
    assert_eq!((out.code, String::from_utf8_lossy(&out.stdout).trim()), (0, "hi"));

    let (_outbox, mut events) = agents::connect_with(&sock, Some(token));
    let connected = block_on(async {
        while let Some(ev) = events.next().await {
            if let agents::Event::Connected { scopes, .. } = ev {
                return scopes;
            }
        }
        panic!("agents never connected");
    });
    assert!(!connected.is_empty());

    assert_eq!(request_to(&endpoint, &json!({"op": "owner.revokeSelf", "id": "r"}), Duration::from_secs(5)).unwrap().ev, "ok");
    assert_eq!(request_to(&endpoint, &status, Duration::from_secs(5)).unwrap_err(), NOT_OWNER);
}
