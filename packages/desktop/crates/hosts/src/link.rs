//! Keeps one ssh tunnel to each host's pocketd up, and reports how it's going.

use crate::Host;
use crate::ssh::{self, Failure};
use futures::channel::mpsc::UnboundedSender;
use serde_json::json;
use std::hash::{BuildHasher, RandomState};
use std::io::{BufRead, BufReader, Read};
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

/// A host's connection, as Settings shows it.
#[derive(Clone, Debug, PartialEq)]
pub enum Link {
    Connecting,
    Online { version: String, uptime: u64 },
    /// Trying again by itself.
    Retrying(Failure),
    /// Waits for Reconnect.
    Failed(Failure),
}

/// Where a link keeps its files.
#[derive(Clone)]
pub struct Paths {
    /// The app's folder on this Mac: tunnel sockets and the app's known_hosts go here.
    pub home: PathBuf,
    /// The app's folder name under the host's home, where its pocketd listens.
    pub remote_dir: String,
}

/// Owns a host's link thread; dropping it closes the tunnel.
pub struct Supervisor {
    tx: Sender<Event>,
}

impl Supervisor {
    /// Reconnects now, without waiting out a retry or a failure.
    pub fn retry(&self) {
        let _ = self.tx.send(Event::Retry);
    }
}

impl Drop for Supervisor {
    fn drop(&mut self) {
        let _ = self.tx.send(Event::Stop);
    }
}

/// Starts connecting to `host`, sending each change to `events` with the host's id.
pub fn supervise(host: Host, paths: Paths, events: UnboundedSender<(String, Link)>) -> Supervisor {
    let (tx, rx) = mpsc::channel();
    let own = tx.clone();
    std::thread::spawn(move || {
        let report = |link| events.unbounded_send((host.id.clone(), link)).is_ok();
        run(&host, &paths, &own, &rx, &report);
        let _ = std::fs::remove_file(host.socket(&paths.home));
    });
    Supervisor { tx }
}

enum Event {
    Retry,
    Stop,
    /// The tunnel numbered this is forwarding.
    Ready(u32),
    Exited(u32),
}

enum End {
    Stop,
    Retry,
    Failed(Failure),
}

/// How long a tunnel may take to forward once ssh signed in.
const READY_WITHIN: Duration = Duration::from_secs(15);
/// How long ssh may take to exit once its forward closed, before that means pocketd isn't there.
const EXIT_WITHIN: Duration = Duration::from_millis(500);
/// How often an open link asks pocketd for its status, which also notices pocketd stopping.
const CHECK_EVERY: Duration = Duration::from_secs(10);

/// Counts failures in a row; a link that reached pocketd starts over, so a flapping host retries quickly.
#[derive(Default)]
struct Backoff {
    attempt: u32,
}

impl Backoff {
    fn reset(&mut self) {
        self.attempt = 0;
    }

    /// The link to show after `failure`, and how long to wait before trying again.
    fn failed(&mut self, failure: Failure) -> (Link, Option<Duration>) {
        let wait = failure.retry_after(self.attempt);
        self.attempt += 1;
        (wait.map_or(Link::Failed(failure), |_| Link::Retrying(failure)), wait)
    }
}

fn run(host: &Host, paths: &Paths, tx: &Sender<Event>, rx: &Receiver<Event>, report: &dyn Fn(Link) -> bool) {
    let (mut backoff, mut tunnel) = (Backoff::default(), 0);
    loop {
        if !report(Link::Connecting) {
            return;
        }
        tunnel += 1;
        let failure = match connect(host, paths, tunnel, tx, rx, report, &mut backoff) {
            End::Stop => return,
            End::Retry => {
                backoff.reset();
                continue;
            }
            End::Failed(failure) => failure,
        };
        let (link, wait) = backoff.failed(failure);
        if !report(link) {
            return;
        }
        match next(rx, None, wait.map(|w| jitter(w, failure))) {
            Some(Event::Stop) => return,
            Some(Event::Retry) => backoff.reset(),
            _ => {}
        }
    }
}

/// Forwards a socket here to pocketd on the host, and checks it until something ends the link.
fn connect(host: &Host, paths: &Paths, tunnel: u32, tx: &Sender<Event>, rx: &Receiver<Event>, report: &dyn Fn(Link) -> bool, backoff: &mut Backoff) -> End {
    let known_hosts = host.known_hosts(&paths.home);
    // Before the probe, or ssh can't record a host key it accepts, and a password sign-in then won't trust it.
    if let Some(dir) = known_hosts.parent() {
        let _ = std::fs::DirBuilder::new().recursive(true).mode(0o700).create(dir);
    }
    let key = Some(host.key(&paths.home)).filter(|k| k.exists());
    let probe = ssh::args(host, &known_hosts, key.as_deref(), &[], &ssh::probe_command(&paths.remote_dir));
    let Ok(out) = Command::new("ssh").args(probe).stdin(Stdio::null()).output() else { return End::Failed(Failure::Other) };
    if !out.status.success() {
        return End::Failed(ssh::classify(&String::from_utf8_lossy(&out.stderr)));
    }
    let Some(found) = ssh::parse_probe(&String::from_utf8_lossy(&out.stdout)) else { return End::Failed(Failure::Other) };
    if !found.running {
        return End::Failed(Failure::NotRunning);
    }
    let local = host.socket(&paths.home);
    let forward = match ssh::forward(&local, &found.home, &paths.remote_dir) {
        Ok(forward) => forward,
        Err(failure) => return End::Failed(failure),
    };
    // stdin stays open for `cat` on the host: if this app dies, the pipe closes and ssh ends with it.
    let spawned = Command::new("ssh")
        .args(ssh::args(host, &known_hosts, key.as_deref(), &forward, ssh::TUNNEL_COMMAND))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let Ok(mut child) = spawned else { return End::Failed(Failure::Other) };
    let (stdout, mut stderr) = (child.stdout.take().unwrap(), child.stderr.take().unwrap());
    let tx = tx.clone();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if line == "READY" {
                let _ = tx.send(Event::Ready(tunnel));
            }
        }
        let _ = tx.send(Event::Exited(tunnel));
    });
    let errors = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = stderr.read_to_string(&mut text);
        text
    });
    let end = watch(&local, tunnel, rx, report, backoff);
    let _ = child.kill();
    let _ = child.wait();
    let stderr = errors.join().unwrap_or_default();
    end.unwrap_or_else(|| End::Failed(ssh::classify(&stderr)))
}

/// Waits for the tunnel, then checks pocketd through it; `None` when ssh exits.
fn watch(local: &Path, tunnel: u32, rx: &Receiver<Event>, report: &dyn Fn(Link) -> bool, backoff: &mut Backoff) -> Option<End> {
    match next(rx, Some(tunnel), Some(READY_WITHIN)) {
        Some(Event::Ready(_)) => {}
        Some(Event::Exited(_)) => return None,
        Some(Event::Stop) => return Some(End::Stop),
        Some(Event::Retry) => return Some(End::Retry),
        None => return Some(End::Failed(Failure::TimedOut)),
    }
    loop {
        match daemon::request_at(local, &json!({"op": "status"})) {
            Ok(reply) => {
                backoff.reset();
                let status = reply.status.unwrap_or_default();
                if !report(Link::Online { version: status.version, uptime: status.uptime }) {
                    return Some(End::Stop);
                }
            }
            // The forward closes at once when nothing listens on the host, but also while ssh exits on a lost link.
            Err(e) if e == daemon::CLOSED => {
                return match next(rx, Some(tunnel), Some(EXIT_WITHIN)) {
                    Some(Event::Exited(_)) => None,
                    Some(Event::Stop) => Some(End::Stop),
                    Some(Event::Retry) => Some(End::Retry),
                    Some(Event::Ready(_)) | None => Some(End::Failed(Failure::NotRunning)),
                };
            }
            // A quiet link times out instead; ServerAlive ends ssh if it's gone, which ends this wait.
            Err(_) => {}
        }
        match next(rx, Some(tunnel), Some(CHECK_EVERY)) {
            Some(Event::Exited(_)) => return None,
            Some(Event::Stop) => return Some(End::Stop),
            Some(Event::Retry) => return Some(End::Retry),
            Some(Event::Ready(_)) | None => {}
        }
    }
}

/// The next event within `wait` (forever when `None`), skipping ones from any tunnel but the open one.
fn next(rx: &Receiver<Event>, open: Option<u32>, wait: Option<Duration>) -> Option<Event> {
    let deadline = wait.map(|w| Instant::now() + w);
    loop {
        let event = match deadline {
            Some(at) => match rx.recv_timeout(at.saturating_duration_since(Instant::now())) {
                Ok(event) => event,
                Err(RecvTimeoutError::Timeout) => return None,
                Err(RecvTimeoutError::Disconnected) => return Some(Event::Stop),
            },
            None => rx.recv().unwrap_or(Event::Stop),
        };
        match event {
            Event::Ready(n) | Event::Exited(n) if open != Some(n) => {}
            event => return Some(event),
        }
    }
}

/// Shortens a wait by up to half, so hosts that dropped together don't retry in step.
/// A throttled host is the exception: it lengthens, since retrying early only extends sshd's penalty.
fn jitter(wait: Duration, failure: Failure) -> Duration {
    // Each `RandomState` gets fresh keys, so this is random enough without a dependency.
    let random = (RandomState::new().hash_one(0u8) % 1000) as f64 / 2000.0;
    wait.mul_f64(if failure == Failure::Throttled { 1.0 + random } else { 1.0 - random })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_link_that_reached_pocketd_retries_quickly_after_it_drops() {
        let mut backoff = Backoff::default();
        let waits: Vec<_> = (0..3).map(|_| backoff.failed(Failure::Refused).1.unwrap().as_secs()).collect();
        assert_eq!(waits, [1, 2, 4]);
        backoff.reset();
        assert_eq!(backoff.failed(Failure::Dropped), (Link::Retrying(Failure::Dropped), Some(Duration::from_secs(1))));
    }

    #[test]
    fn a_refused_login_waits_for_reconnect() {
        assert_eq!(Backoff::default().failed(Failure::Auth), (Link::Failed(Failure::Auth), None));
    }

    #[test]
    fn events_from_a_closed_tunnel_are_skipped() {
        let (tx, rx) = mpsc::channel();
        tx.send(Event::Exited(1)).unwrap();
        tx.send(Event::Ready(2)).unwrap();
        assert!(matches!(next(&rx, Some(2), Some(Duration::ZERO)), Some(Event::Ready(2))));
        tx.send(Event::Exited(2)).unwrap();
        assert!(next(&rx, None, Some(Duration::from_millis(1))).is_none(), "a closed tunnel's exit doesn't cut a retry wait short");
        drop(tx);
        assert!(matches!(next(&rx, None, None), Some(Event::Stop)));
    }

    #[test]
    fn hosts_that_dropped_together_retry_at_different_times() {
        let waits: Vec<_> = (0..20).map(|_| jitter(Duration::from_secs(8), Failure::Dropped)).collect();
        assert!(waits.iter().all(|w| *w > Duration::from_secs(4) && *w <= Duration::from_secs(8)));
        assert!(waits.iter().any(|w| *w != waits[0]));
    }

    #[test]
    fn a_throttled_host_never_retries_before_its_penalty_ends() {
        assert!((0..20).all(|_| jitter(Duration::from_secs(20), Failure::Throttled) >= Duration::from_secs(20)));
    }
}
