use crate::caps::{CAPS, DELIVERY_CAP};
use crate::event::{Frame, automation_event, board_event, delegation_event, host_event, launch_event, names_event, registry_event};
use crate::ids::PAIR;
use crate::linklog::LinkLog;
use crate::resend::{Resend, is_prompt};
use crate::timelines::Timelines;
use crate::{Event, Outbox, hosts, queue};
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::io::ErrorKind;
#[cfg(unix)]
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::time::{Duration, Instant};
use tungstenite::Message;
#[cfg(windows)]
use uds_windows::UnixStream;

/// Why a stop or queue change the socket never took failed.
const UNSENT: &str = "Lost the connection to pocketd";
/// How long a pocketd may take to answer the websocket handshake, and to take each write after it, before a host's link counts as lost.
const STALL: Duration = Duration::from_secs(5);
const RETRY: Duration = Duration::from_secs(2);

/// What lies behind the socket, which decides whether a quiet pocketd counts as a lost link.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Peer {
    /// This Mac's pocketd. A busy Mac makes it slow, not gone, and a pocketd that died closes the socket, so no timer applies.
    ThisMac,
    /// A host's pocketd behind an SSH forward, which can go quiet with the socket still open: a handshake or write that takes over `STALL` loses the link.
    Host,
}

impl Peer {
    fn stall(self) -> Option<Duration> {
        match self {
            Peer::ThisMac => None,
            Peer::Host => Some(STALL),
        }
    }
}

/// Follows pocketd's phone protocol over its unix socket, where pocketd tells
/// the owner from a process inside a Terminal: the agent list, and the timelines the app asks for.
pub fn connect(sock: &Path) -> (Outbox, UnboundedReceiver<Event>) {
    connect_with(sock, None, Peer::ThisMac, None)
}

/// `connect` to a socket behind which pocketd wants an owner token: a Windows host's forwarded TCP port.
pub fn connect_with(sock: &Path, token: Option<String>, peer: Peer, log: Option<LinkLog>) -> (Outbox, UnboundedReceiver<Event>) {
    let (tx, rx) = unbounded();
    let (out, queue) = channel();
    let sock = sock.to_path_buf();
    std::thread::spawn(move || {
        let name = sock.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        let mut carry = Carry::new(name, log);
        loop {
            let _ = run(&sock, token.as_deref(), &tx, &queue, &mut carry, peer.stall());
            carry.dropped(&tx);
            if tx.is_closed() {
                return;
            }
            carry.wait_out(&queue, &tx, RETRY);
        }
    });
    (Outbox(out), rx)
}

fn hello(token: Option<&str>) -> Value {
    let mut hello = json!({"type": "hello", "id": "h", "clientId": "desktop", "protocolVersion": 3, "caps": CAPS});
    if let Some(token) = token {
        hello["ownerToken"] = token.into();
    }
    hello
}

/// What the link thread keeps from one connection to the next.
struct Carry {
    unanswered: Vec<Value>,
    requests: Requests,
    resend: Resend,
    timelines: Timelines,
    held: VecDeque<Value>,
    /// Whether `hello.ok` has come on the connection that dropped last, so a drop is logged once, not once per retry.
    up: bool,
    name: String,
    log: Option<LinkLog>,
}

impl Carry {
    fn new(name: String, log: Option<LinkLog>) -> Self {
        Carry {
            unanswered: Vec::new(),
            requests: Requests::default(),
            resend: Resend::new(Instant::now()),
            timelines: Timelines::default(),
            held: VecDeque::new(),
            up: false,
            name,
            log,
        }
    }

    fn note(&self, text: &str) {
        if let Some(log) = &self.log {
            log.line(&format!("{}: {text}", self.name));
        }
    }

    fn report(&mut self, tx: &UnboundedSender<Event>, events: Vec<Event>) -> Option<()> {
        for line in self.resend.notes() {
            self.note(&line);
        }
        events.into_iter().try_for_each(|ev| tx.unbounded_send(ev).ok())
    }

    fn dropped(&mut self, tx: &UnboundedSender<Event>) {
        if std::mem::take(&mut self.up) {
            self.note("link down");
        }
        let mut events = self.resend.lost(Instant::now());
        events.extend(self.requests.refuse_all(UNSENT));
        let _ = self.report(tx, events);
    }

    fn wait_out(&mut self, queue: &Receiver<Value>, tx: &UnboundedSender<Event>, span: Duration) {
        let until = Instant::now() + span;
        loop {
            let left = until.saturating_duration_since(Instant::now());
            match queue.recv_timeout(left) {
                Ok(m) => {
                    let events = if is_prompt(&m) { self.resend.hold(m.clone(), Instant::now()) } else { Vec::new() };
                    self.held.push_back(m);
                    let _ = self.report(tx, events);
                }
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => {
                    std::thread::sleep(left);
                    break;
                }
            }
        }
        let events = self.resend.expire(Instant::now());
        let _ = self.report(tx, events);
    }
}

fn run(sock: &Path, token: Option<&str>, tx: &UnboundedSender<Event>, queue: &Receiver<Value>, carry: &mut Carry, stall: Option<Duration>) -> Option<()> {
    let stream = UnixStream::connect(sock).ok()?;
    stream.set_read_timeout(stall).ok()?;
    stream.set_write_timeout(stall).ok()?;
    let (mut ws, _) = tungstenite::client("ws://localhost/", stream).ok()?;
    // Reads time out so the loop gets to send the queue even while pocketd is quiet; the timeout is as long as a
    // send can wait, so it stays short.
    ws.get_ref().set_read_timeout(Some(Duration::from_millis(10))).ok()?;
    ws.send(Message::text(hello(token).to_string())).ok()?;
    for m in carry.unanswered.iter() {
        ws.send(Message::text(m.to_string())).ok()?;
    }
    for m in carry.timelines.again() {
        ws.send(Message::text(m.to_string())).ok()?;
    }
    // Prompts the drop left in doubt go first, once `hello.ok` says whether resending them is safe.
    let mut ready = !carry.resend.waiting();
    loop {
        let expired = carry.resend.expire(Instant::now());
        carry.report(tx, expired)?;
        let resend = &carry.resend;
        carry.held.retain(|m| !is_prompt(m) || resend.holds(m));
        while ready && let Some(m) = carry.held.pop_front().or_else(|| queue.try_recv().ok()) {
            let Some(m) = carry.timelines.ask(m) else { continue };
            if m["type"] == "agent.create" {
                carry.unanswered.push(m.clone());
            }
            if is_prompt(&m) {
                carry.resend.sent(&m);
            }
            if ws.send(Message::text(m.to_string())).is_err() {
                if let Some(ev) = refused(&m, UNSENT).filter(|_| !is_prompt(&m)) {
                    let _ = tx.unbounded_send(ev);
                }
                return None;
            }
            carry.requests.sent(&m);
        }
        let raw = match ws.read() {
            Ok(Message::Text(raw)) => raw,
            Ok(_) => continue,
            Err(tungstenite::Error::Io(e)) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => continue,
            Err(_) => return None,
        };
        let Ok(mut f) = serde_json::from_str::<Frame>(&raw) else { continue };
        match f.kind.as_str() {
            "agent.timeline" => {
                if let Some(ev) = carry.timelines.reply(&mut f) {
                    carry.report(tx, vec![ev])?;
                }
                continue;
            }
            "error" => carry.timelines.refused(&f.id),
            _ => {}
        }
        if !matches!(f.kind.as_str(), "agent.creating" | "agent.progress") {
            carry.unanswered.retain(|m| m["id"] != f.id.as_str());
        }
        if let Some(ev) = carry
            .resend
            .answer(&f)
            .or_else(|| carry.requests.answer(&f))
            .or_else(|| launch_event(&f))
            .or_else(|| names_event(&f))
            .or_else(|| automation_event(&f))
            .or_else(|| board_event(&f))
            .or_else(|| registry_event(&f, &raw))
            .or_else(|| crate::worktrees::decode(&f.kind, &raw))
            .or_else(|| hosts::decode(&f.kind, &raw))
            .or_else(|| delegation_event(&f.kind, &raw))
            .or_else(|| queue::decode(&f.kind, &raw).map(|(agent, entries)| Event::Queue { agent, entries }))
        {
            carry.report(tx, vec![ev])?;
            continue;
        }
        let mut asks: Vec<Value> = Vec::new();
        let ev = match f.kind.as_str() {
            "hello.ok" => {
                let host = host_event(&f);
                let receipts = f.caps.iter().any(|cap| cap == DELIVERY_CAP);
                tx.unbounded_send(Event::Connected { scopes: f.scopes, caps: f.caps, version: f.version }).ok()?;
                let (again, events) = carry.resend.linked(receipts, Instant::now());
                for m in &again {
                    ws.send(Message::text(m.to_string())).ok()?;
                }
                carry.up = true;
                carry.note("link up");
                carry.report(tx, events)?;
                ready = true;
                let Some(ev) = host else { continue };
                ev
            }
            "agent.list" => {
                asks.extend(f.agents.iter().filter_map(|a| carry.timelines.seen(a)));
                Event::Agents(f.agents)
            }
            "agent.update" => {
                let Some(a) = f.agent else { continue };
                asks.extend(carry.timelines.seen(&a));
                Event::Agent(Box::new(a))
            }
            "agent.stream" => {
                if let Some(item) = &f.item {
                    carry.timelines.streamed(&f.agent_id, item);
                }
                Event::Items(f.agent_id, f.item.into_iter().collect())
            }
            "permission.request" => {
                let Some(r) = f.request else { continue };
                Event::Asked(r)
            }
            "permission.resolved" => Event::Resolved(f.request_id),
            "pair.offer" => Event::PairCode { url: f.url, code: f.code, expires_at: f.expires_at },
            "pair.done" => Event::Paired(f.name),
            "error" if f.id == PAIR => Event::PairFailed(f.message),
            _ => match host_event(&f) {
                Some(ev) => ev,
                None => continue,
            },
        };
        tx.unbounded_send(ev).ok()?;
        for m in asks {
            ws.send(Message::text(m.to_string())).ok()?;
        }
    }
}

/// Stops and queue changes written to pocketd and not yet answered, so a refusal names the one it refused. Prompts
/// are `Resend`'s.
#[derive(Default)]
struct Requests(VecDeque<Value>);

impl Requests {
    fn sent(&mut self, m: &Value) {
        if matches!(m["type"].as_str(), Some("agent.interrupt" | "queue.edit" | "queue.cancel" | "queue.reorder" | "queue.send_now")) {
            self.0.push_back(m.clone());
        }
    }

    /// Drops the request `f` answers; a refusal becomes `InterruptFailed` or `QueueFailed`.
    fn answer(&mut self, f: &Frame) -> Option<Event> {
        if !matches!(f.kind.as_str(), "ack" | "error") {
            return None;
        }
        let i = self.0.iter().position(|m| m["id"] == f.id.as_str())?;
        let m = self.0.remove(i)?;
        if f.kind == "error" { refused(&m, &f.message) } else { None }
    }

    fn refuse_all(&mut self, reason: &str) -> Vec<Event> {
        self.0.drain(..).filter_map(|m| refused(&m, reason)).collect()
    }
}

/// The event for a prompt, stop or queue change `m` that failed for `reason`; none for other messages.
pub(super) fn refused(m: &Value, reason: &str) -> Option<Event> {
    let field = |key: &str| m[key].as_str().unwrap_or_default().to_string();
    let queue_failed = |text: Option<String>| Some(Event::QueueFailed { agent: field("agentId"), reason: reason.into(), text });
    match m["type"].as_str()? {
        "agent.prompt" if m["queue"] == true => queue_failed(Some(field("text"))),
        "queue.edit" => queue_failed(Some(field("text"))),
        "queue.cancel" | "queue.reorder" | "queue.send_now" => queue_failed(None),
        "agent.prompt" => Some(Event::PromptFailed { agent: field("agentId"), request: field("id"), reason: reason.into() }),
        "agent.interrupt" => Some(Event::InterruptFailed { agent: field("agentId"), reason: reason.into() }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Agents;
    use crate::Host;
    use crate::Local;
    use crate::PromptState;
    use crate::resend::DOWN_LIMIT;
    #[cfg(unix)]
    use std::os::unix::net::UnixListener;
    use std::path::PathBuf;
    #[cfg(windows)]
    use uds_windows::UnixListener;

    type Ws = tungstenite::WebSocket<UnixStream>;

    fn sock_path(name: &str) -> PathBuf {
        let sock = PathBuf::from("/tmp").join(format!("pa-{name}-{}.sock", std::process::id()));
        let _ = std::fs::remove_file(&sock);
        sock
    }

    fn pocketd(name: &str) -> (UnixListener, PathBuf) {
        let sock = sock_path(name);
        (UnixListener::bind(&sock).unwrap(), sock)
    }

    fn accept(server: &UnixListener) -> Ws {
        let (peer, _) = server.accept().unwrap();
        peer.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        tungstenite::accept(peer).unwrap()
    }

    fn read(ws: &mut Ws) -> Value {
        serde_json::from_str(ws.read().unwrap().to_text().unwrap()).unwrap()
    }

    fn frame(raw: Value) -> Frame {
        serde_json::from_value(raw).unwrap()
    }

    fn welcome(ws: &mut Ws, receipts: bool) {
        assert_eq!(read(ws)["type"], "hello");
        let caps: &[&str] = if receipts { &["delivery.v1"] } else { &[] };
        ws.send(Message::text(json!({"type": "hello.ok", "caps": caps}).to_string())).unwrap();
    }

    fn next_matching<T>(events: &mut impl Iterator<Item = Event>, pick: impl Fn(Event) -> Option<T>) -> T {
        events.find_map(pick).unwrap()
    }

    #[test]
    fn the_hello_carries_an_owner_token_only_when_there_is_one() {
        assert!(hello(None).get("ownerToken").is_none());
        assert_eq!(hello(Some("t1"))["ownerToken"], "t1");
    }

    #[test]
    fn a_pocketd_that_never_answers_the_handshake_is_given_up_on() {
        let (_server, sock) = pocketd("silent");
        let (tx, _events) = unbounded();
        let (_out, queue) = channel();
        let (done_tx, done) = std::sync::mpsc::channel();
        std::thread::spawn(move || done_tx.send(run(&sock, None, &tx, &queue, &mut Carry::new(String::new(), None), Some(Duration::from_millis(200)))));
        assert_eq!(done.recv_timeout(Duration::from_secs(2)), Ok(None));
    }

    #[test]
    fn sends_queued_messages_while_pocketd_is_quiet() {
        let (server, sock) = pocketd("queue");
        let (out, _events) = connect(&sock);
        let mut ws = accept(&server);

        assert_eq!(
            read(&mut ws),
            json!({"type": "hello", "id": "h", "clientId": "desktop", "protocolVersion": 3, "caps": ["pair.v1", "scopes.v1", "summary.v2", "host.v1", "open.v1", "names.v1", "automations.v1", "board.v1", "locals.v1", "fork.v1", "accounts.v1", "subagents.v1", "settings.v1", "commands.v1", "access.v1", "skills.v1", "orchestrator.v1", "hosts.v1", "registry.v2", "workspace.v1", "worktrees.v1", "delivery.v1"]})
        );
        out.view(&["a1".into()]);
        assert_eq!(read(&mut ws), json!({"type": "agent.view", "id": "view", "agentIds": ["a1"]}));
        out.seen(&["a1".into()]);
        assert_eq!(read(&mut ws), json!({"type": "agent.seen", "id": "seen", "agentIds": ["a1"]}));
        std::fs::remove_file(&sock).unwrap();
    }

    fn page(id: &str, seqs: std::ops::RangeInclusive<i64>, older: bool) -> Message {
        let items: Vec<Value> = seqs.map(|s| json!({"id": format!("i{s}"), "seq": s, "kind": "assistant"})).collect();
        Message::text(json!({"type": "agent.timeline", "id": id, "agentId": "a1", "hasOlder": older, "maxSeq": 6, "items": items}).to_string())
    }

    fn next_after_seen(out: &Outbox, ws: &mut Ws) -> Value {
        out.seen(&[]);
        read(ws)
    }

    fn listed(ws: &mut Ws) {
        ws.send(Message::text(json!({"type": "agent.list", "agents": [{"id": "a1"}, {"id": "a2"}]}).to_string())).unwrap();
    }

    #[test]
    fn connecting_asks_no_timeline() {
        let (server, sock) = pocketd("no-timeline");
        let (out, _events) = connect(&sock);
        let mut ws = accept(&server);
        read(&mut ws);
        listed(&mut ws);
        ws.send(Message::text(json!({"type": "agent.update", "agent": {"id": "a3"}}).to_string())).unwrap();
        assert_eq!(next_after_seen(&out, &mut ws)["type"], "agent.seen");
        std::fs::remove_file(&sock).unwrap();
    }

    #[test]
    fn opening_a_chat_asks_its_newest_page_once() {
        let (server, sock) = pocketd("newest");
        let (out, _events) = connect(&sock);
        let mut ws = accept(&server);
        read(&mut ws);
        out.timeline("a1");
        out.timeline("a1");
        assert_eq!(read(&mut ws), json!({"type": "agent.timeline", "id": "t-a1", "agentId": "a1", "limit": 500}));
        ws.send(page("t-a1", 5..=6, true)).unwrap();
        out.timeline("a1");
        assert_eq!(next_after_seen(&out, &mut ws)["type"], "agent.seen");
        std::fs::remove_file(&sock).unwrap();
    }

    #[test]
    fn scrolling_to_the_top_asks_for_an_older_page_only_where_pocketd_said_there_is_one() {
        let (server, sock) = pocketd("older");
        let (out, events) = connect(&sock);
        let (mut events, mut a) = (futures::executor::block_on_stream(events), Agents::default());
        let mut ws = accept(&server);
        read(&mut ws);
        out.older("a1");
        assert_eq!(next_after_seen(&out, &mut ws)["type"], "agent.seen", "nothing is loaded to be above");
        out.timeline("a1");
        read(&mut ws);
        ws.send(page("t-a1", 5..=6, true)).unwrap();
        a.apply(events.next().unwrap());
        out.older("a1");
        assert_eq!(read(&mut ws), json!({"type": "agent.timeline", "id": "o-a1", "agentId": "a1", "limit": 500, "beforeSeq": 5}));
        out.older("a1");
        assert_eq!(next_after_seen(&out, &mut ws)["type"], "agent.seen", "one was on its way");
        ws.send(page("o-a1", 3..=4, true)).unwrap();
        a.apply(events.next().unwrap());
        out.older("a1");
        assert_eq!(read(&mut ws)["beforeSeq"], 3);
        ws.send(page("o-a1", 1..=2, false)).unwrap();
        a.apply(events.next().unwrap());
        out.older("a1");
        assert_eq!(next_after_seen(&out, &mut ws)["type"], "agent.seen", "pocketd said that was the first");

        assert_eq!(a.timelines["a1"].iter().map(|i| i.seq).collect::<Vec<_>>(), [1, 2, 3, 4, 5, 6]);
        std::fs::remove_file(&sock).unwrap();
    }

    #[test]
    fn a_pocketd_that_ignores_before_seq_is_not_asked_again() {
        let (server, sock) = pocketd("ignores");
        let (out, events) = connect(&sock);
        let mut events = futures::executor::block_on_stream(events);
        let mut ws = accept(&server);
        read(&mut ws);
        out.timeline("a1");
        read(&mut ws);
        ws.send(page("t-a1", 5..=6, true)).unwrap();
        events.next();
        out.older("a1");
        assert_eq!(read(&mut ws)["beforeSeq"], 5);
        ws.send(page("o-a1", 5..=6, true)).unwrap();
        events.next();
        out.older("a1");
        assert_eq!(next_after_seen(&out, &mut ws)["type"], "agent.seen", "asked for the same page again");
        std::fs::remove_file(&sock).unwrap();
    }

    #[test]
    fn a_reconnect_asks_only_what_is_newer_for_the_timelines_loaded_and_merges_it() {
        let (server, sock) = pocketd("since");
        let (out, events) = connect(&sock);
        let (mut events, mut a) = (futures::executor::block_on_stream(events), Agents::default());
        let mut ws = accept(&server);
        read(&mut ws);
        listed(&mut ws);
        out.timeline("a1");
        read(&mut ws);
        ws.send(page("t-a1", 1..=6, false)).unwrap();
        (0..2).for_each(|_| a.apply(events.next().unwrap()));
        drop(ws);

        let mut ws = accept(&server);
        read(&mut ws);
        assert_eq!(read(&mut ws), json!({"type": "agent.timeline", "id": "s-a1", "agentId": "a1", "limit": 500, "sinceSeq": 5}));
        assert_eq!(next_after_seen(&out, &mut ws)["type"], "agent.seen", "a2 was never opened, so it isn't asked");
        ws.send(page("s-a1", 6..=8, true)).unwrap();
        a.apply(events.next().unwrap());
        assert_eq!(a.timelines["a1"].iter().map(|i| i.seq).collect::<Vec<_>>(), [1, 2, 3, 4, 5, 6, 7, 8], "the older items stay");
        std::fs::remove_file(&sock).unwrap();
    }

    #[test]
    fn a_newest_page_asked_when_the_link_dropped_is_asked_again() {
        let (server, sock) = pocketd("owed");
        let (out, _events) = connect(&sock);
        let mut ws = accept(&server);
        read(&mut ws);
        out.timeline("a1");
        read(&mut ws);
        drop(ws);

        let mut ws = accept(&server);
        read(&mut ws);
        assert_eq!(read(&mut ws), json!({"type": "agent.timeline", "id": "t-a1", "agentId": "a1", "limit": 500}));
        std::fs::remove_file(&sock).unwrap();
    }

    #[test]
    fn connected_carries_the_scopes_from_hello_ok() {
        let (server, sock) = pocketd("scopes");
        let (_out, events) = connect(&sock);
        let mut ws = accept(&server);
        read(&mut ws);
        let ok = include_str!("../../../../pocketd/internal/proto/testdata/golden/server/hello_ok_scopes.json");
        ws.send(Message::text(ok)).unwrap();

        let mut a = Agents::default();
        a.apply(futures::executor::block_on(futures::StreamExt::into_future(events)).0.unwrap());
        assert!(a.owner() && !a.observe_only() && a.pairing());
        a.apply(Event::Connected { scopes: vec!["observe".into()], caps: vec![], version: String::new() });
        assert!(a.observe_only() && !a.pairing());
        std::fs::remove_file(&sock).unwrap();
    }

    #[test]
    fn connected_carries_pocketds_build_from_hello_ok() {
        let (server, sock) = pocketd("version");
        let (_out, events) = connect(&sock);
        let mut ws = accept(&server);
        read(&mut ws);
        let ok = include_str!("../../../../pocketd/internal/proto/testdata/golden/server/hello_ok_version.json");
        ws.send(Message::text(ok)).unwrap();
        let ev = futures::executor::block_on(futures::StreamExt::into_future(events)).0.unwrap();
        assert!(matches!(ev, Event::Connected { version, .. } if version == "0.1.0"));
        std::fs::remove_file(&sock).unwrap();
    }

    #[test]
    fn the_host_from_hello_ok_outlives_the_connect() {
        let (server, sock) = pocketd("host");
        let (_out, events) = connect(&sock);
        let mut ws = accept(&server);
        read(&mut ws);
        let ok = include_str!("../../../../pocketd/internal/proto/testdata/golden/server/hello_ok_host.json");
        let changed = include_str!("../../../../pocketd/internal/proto/testdata/golden/server/host_changed.json");
        ws.send(Message::text(ok)).unwrap();

        let mut a = Agents::default();
        let mut got = futures::executor::block_on_stream(events);
        for ev in got.by_ref().take(2) {
            a.apply(ev);
        }
        assert_eq!(a.host, Some(Host { tailnet: true, keeping_awake: false, ..Default::default() }));
        ws.send(Message::text(changed)).unwrap();
        a.apply(got.next().unwrap());
        assert_eq!(a.host, Some(Host { tailnet: true, keeping_awake: true, ..Default::default() }));
        std::fs::remove_file(&sock).unwrap();
    }

    #[test]
    fn pairing_frames_become_pair_events() {
        let (server, sock) = pocketd("pair");
        let (out, events) = connect(&sock);
        let mut ws = accept(&server);
        read(&mut ws);
        out.pair_begin();
        assert_eq!(read(&mut ws), json!({"type": "pair.begin", "id": "pair"}));
        let offer = include_str!("../../../../pocketd/internal/proto/testdata/golden/server/pair_offer.json");
        let done = include_str!("../../../../pocketd/internal/proto/testdata/golden/server/pair_done.json");
        for raw in [offer, done] {
            ws.send(Message::text(raw)).unwrap();
        }
        let off = json!({"type": "error", "id": "pair", "message": "Tailscale isn't running. Phones can't reach this Mac.", "code": "tailnet_off"});
        ws.send(Message::text(off.to_string())).unwrap();

        let got: Vec<Event> = futures::executor::block_on_stream(events).take(3).collect();
        assert!(matches!(&got[0], Event::PairCode { code, expires_at: 1790000000000, .. } if code == "abcdefghijklmnopqrstuv"));
        assert!(matches!(&got[1], Event::Paired(name) if name == "iPhone"));
        assert!(matches!(&got[2], Event::PairFailed(m) if m == "Tailscale isn't running. Phones can't reach this Mac."));
        std::fs::remove_file(&sock).unwrap();
    }

    #[test]
    fn create_sends_the_spec_under_a_fresh_request_id() {
        let (server, sock) = pocketd("create");
        let (out, _events) = connect(&sock);
        let mut ws = accept(&server);
        read(&mut ws);
        let spec = json!({"project": "/p", "checkout": {"worktree": "/p"}, "provider": "claude", "access": "ask", "plan": false});
        let (a, b) = (out.create(spec.clone()), out.create(spec.clone()));
        assert_ne!(a, b);
        assert_eq!(read(&mut ws), json!({"type": "agent.create", "id": format!("create-{a}"), "requestId": a, "spec": spec}));
        read(&mut ws);
        std::fs::remove_file(&sock).unwrap();
    }

    #[test]
    fn a_create_is_resent_after_each_reconnect_until_its_final_reply() {
        let (server, sock) = pocketd("resend");
        let (out, events) = connect(&sock);
        let mut ws = accept(&server);
        read(&mut ws);
        let request = out.create(json!({"provider": "claude"}));
        let create = read(&mut ws);
        drop(ws);

        let mut ws = accept(&server);
        read(&mut ws);
        assert_eq!(read(&mut ws), create);
        let creating = json!({"type": "agent.creating", "id": format!("create-{request}"), "requestId": request, "terminalId": "t1"});
        ws.send(Message::text(creating.to_string())).unwrap();
        let progress = json!({"type": "agent.progress", "id": format!("create-{request}"), "requestId": request, "step": "setup"});
        ws.send(Message::text(progress.to_string())).unwrap();
        let mut events = futures::executor::block_on_stream(events);
        assert!(matches!(events.next(), Some(Event::Creating { .. })));
        assert!(matches!(events.next(), Some(Event::Progress { step, .. }) if step == "setup"));
        drop(ws);

        let mut ws = accept(&server);
        read(&mut ws);
        assert_eq!(read(&mut ws), create);
        let failed = json!({"type": "error", "id": format!("create-{request}"), "code": "spawn_failed", "message": "Setup exited 1"});
        ws.send(Message::text(failed.to_string())).unwrap();
        assert!(matches!(events.next(), Some(Event::CreateFailed { code, .. }) if code == "spawn_failed"));
        drop(ws);

        let mut ws = accept(&server);
        read(&mut ws);
        out.seen(&[]);
        assert_eq!(read(&mut ws)["type"], "agent.seen");
        std::fs::remove_file(&sock).unwrap();
    }

    #[test]
    fn a_refused_stop_reports_its_agent_and_why() {
        let mut requests = Requests::default();
        requests.sent(&json!({"type": "agent.interrupt", "id": "interrupt-1", "agentId": "a1"}));
        requests.sent(&json!({"type": "agent.interrupt", "id": "interrupt-2", "agentId": "a2"}));
        assert!(requests.answer(&frame(json!({"type": "ack", "id": "interrupt-1"}))).is_none());
        let refused =
            requests.answer(&frame(json!({"type": "error", "id": "interrupt-2", "code": "agent_resuming", "message": "This session is still resuming"})));
        assert!(matches!(refused, Some(Event::InterruptFailed { agent, reason }) if agent == "a2" && reason == "This session is still resuming"));
    }

    #[test]
    fn a_refused_queue_change_names_its_session_and_gives_back_the_text() {
        let (tx, rx) = std::sync::mpsc::channel();
        let out = Outbox::from(tx);
        out.queue_edit("a1", "e1", "fix lint too");
        out.queue_cancel("a1", "e2");
        out.queue_reorder("a1", "e3", "e1");
        out.queue_send_now("a1", "e4");
        let mut requests = Requests::default();
        let sent: Vec<Value> = rx.try_iter().collect();
        sent.iter().for_each(|m| requests.sent(m));
        let refuse = |requests: &mut Requests, m: &Value| requests.answer(&frame(json!({"type": "error", "id": m["id"], "message": "No such entry"})));
        let got: Vec<_> = sent.iter().map(|m| refuse(&mut requests, m)).collect();
        assert!(
            matches!(&got[0], Some(Event::QueueFailed { agent, reason, text }) if agent == "a1" && reason == "No such entry" && text.as_deref() == Some("fix lint too"))
        );
        assert!(matches!(&got[1], Some(Event::QueueFailed { agent, text: None, .. }) if agent == "a1"));
        assert!(matches!(&got[2], Some(Event::QueueFailed { text: None, .. })));
        assert!(matches!(&got[3], Some(Event::QueueFailed { text: None, .. })));
    }

    #[test]
    fn an_acked_queue_change_reports_nothing_and_one_the_socket_never_took_fails() {
        let (tx, rx) = std::sync::mpsc::channel();
        let out = Outbox::from(tx);
        out.queue_cancel("a1", "e1");
        let cancel: Value = rx.try_recv().unwrap();
        let mut requests = Requests::default();
        requests.sent(&cancel);
        assert!(requests.answer(&frame(json!({"type": "ack", "id": cancel["id"]}))).is_none());
        assert!(matches!(refused(&cancel, UNSENT), Some(Event::QueueFailed { agent, reason, .. }) if agent == "a1" && reason == UNSENT));
    }

    #[test]
    fn a_prompt_or_stop_the_socket_never_took_fails_and_nothing_else_does() {
        let unsent = json!({"type": "agent.prompt", "id": "prompt-1", "agentId": "a1", "text": "Fix it"});
        assert!(
            matches!(refused(&unsent, UNSENT), Some(Event::PromptFailed { agent, request, reason }) if agent == "a1" && request == "prompt-1" && reason == UNSENT)
        );
        let stop = json!({"type": "agent.interrupt", "id": "interrupt-1", "agentId": "a1"});
        assert!(matches!(refused(&stop, UNSENT), Some(Event::InterruptFailed { agent, reason }) if agent == "a1" && reason == UNSENT));
        assert!(refused(&json!({"type": "agent.compact", "id": "compact", "agentId": "a1"}), UNSENT).is_none());
    }

    #[test]
    fn each_prompt_goes_out_under_its_own_id_and_its_refusal_reaches_the_app() {
        let (server, sock) = pocketd("prompt");
        let (out, events) = connect(&sock);
        let mut ws = accept(&server);
        read(&mut ws);
        let first = out.prompt("a1", "Fix it");
        let second = out.prompt("a1", "Fix it");
        assert_ne!(first, second);
        assert_eq!((read(&mut ws)["id"].as_str(), read(&mut ws)["id"].as_str()), (Some(first.as_str()), Some(second.as_str())));
        let refusal = json!({"type": "error", "id": second, "message": "Agent isn't in the foreground"});
        ws.send(Message::text(refusal.to_string())).unwrap();
        ws.send(Message::text(json!({"type": "agent.update", "agent": {"id": "a1"}}).to_string())).unwrap();
        let mut events = futures::executor::block_on_stream(events);
        assert!(
            matches!(events.next(), Some(Event::PromptFailed { agent, request, reason }) if agent == "a1" && request == second && reason == "Agent isn't in the foreground")
        );
        std::fs::remove_file(&sock).unwrap();
    }

    #[test]
    fn a_stop_or_queue_change_still_unanswered_when_the_link_drops_fails_though_receipts_are_kept() {
        let (server, sock) = pocketd("dropped");
        let (out, events) = connect(&sock);
        let mut ws = accept(&server);
        welcome(&mut ws, true);
        out.interrupt("a2");
        out.queue_cancel("a1", "e1");
        assert_eq!(
            (read(&mut ws)["type"].as_str().map(str::to_string), read(&mut ws)["type"].as_str().map(str::to_string)),
            (Some("agent.interrupt".into()), Some("queue.cancel".into()))
        );
        drop(ws);
        let mut events = futures::executor::block_on_stream(events);
        assert!(
            matches!(next_matching(&mut events, |ev| matches!(ev, Event::InterruptFailed { .. }).then_some(ev)), Event::InterruptFailed { agent, reason } if agent == "a2" && reason == UNSENT)
        );
        assert!(matches!(events.next(), Some(Event::QueueFailed { agent, .. }) if agent == "a1"));
        std::fs::remove_file(&sock).unwrap();
    }

    #[test]
    fn an_unacked_prompt_is_sent_again_under_its_id_after_a_reconnect_that_keeps_receipts() {
        let (server, sock) = pocketd("again");
        let (out, events) = connect(&sock);
        let mut ws = accept(&server);
        welcome(&mut ws, true);
        let id = out.prompt("a1", "Fix it");
        let sent = read(&mut ws);
        assert_eq!(sent["id"], id.as_str());
        drop(ws);

        let mut ws = accept(&server);
        welcome(&mut ws, true);
        assert_eq!(read(&mut ws), sent);
        let mut events = futures::executor::block_on_stream(events);
        let resending = next_matching(&mut events, |ev| match ev {
            Event::PromptState { request, state, .. } => Some((request, state)),
            _ => None,
        });
        assert_eq!(resending, (id.clone(), PromptState::Resending));
        std::fs::remove_file(&sock).unwrap();
    }

    #[test]
    fn an_acked_prompt_with_no_delivery_is_sent_again_after_a_reconnect() {
        let (server, sock) = pocketd("acked");
        let (out, events) = connect(&sock);
        let mut ws = accept(&server);
        welcome(&mut ws, true);
        let id = out.prompt("a1", "Fix it");
        let sent = read(&mut ws);
        ws.send(Message::text(json!({"type": "ack", "id": id}).to_string())).unwrap();
        let mut events = futures::executor::block_on_stream(events);
        next_matching(&mut events, |ev| matches!(ev, Event::PromptState { state: PromptState::Delivered, .. }).then_some(()));
        drop(ws);

        let mut ws = accept(&server);
        welcome(&mut ws, true);
        assert_eq!(read(&mut ws), sent);
        std::fs::remove_file(&sock).unwrap();
    }

    #[test]
    fn a_prompt_is_not_sent_again_when_the_link_never_had_receipts_and_fails() {
        let (server, sock) = pocketd("nocap");
        let (out, events) = connect(&sock);
        let mut ws = accept(&server);
        welcome(&mut ws, false);
        let id = out.prompt("a1", "Fix it");
        read(&mut ws);
        drop(ws);
        let mut events = futures::executor::block_on_stream(events);
        let reason = next_matching(&mut events, |ev| match ev {
            Event::PromptFailed { request, reason, .. } if request == id => Some(reason),
            _ => None,
        });
        assert_eq!(reason, "can't reach Anywhere's service");

        let mut ws = accept(&server);
        welcome(&mut ws, false);
        ws.get_ref().set_read_timeout(Some(Duration::from_millis(500))).unwrap();
        assert!(ws.read().is_err(), "a prompt pocketd may have taken went out again");
        std::fs::remove_file(&sock).unwrap();
    }

    #[test]
    fn a_failed_delivery_reaches_the_app_with_pocketds_reason() {
        let (server, sock) = pocketd("failed");
        let (out, events) = connect(&sock);
        let mut ws = accept(&server);
        welcome(&mut ws, true);
        let id = out.prompt("a1", "Fix it");
        read(&mut ws);
        ws.send(Message::text(json!({"type": "ack", "id": id}).to_string())).unwrap();
        let failed = json!({"type": "agent.delivery", "id": id, "agentId": "a1", "state": "failed", "error": "This session is still resuming"});
        ws.send(Message::text(failed.to_string())).unwrap();
        let mut events = futures::executor::block_on_stream(events);
        let reason = next_matching(&mut events, |ev| match ev {
            Event::PromptFailed { agent, request, reason } if agent == "a1" && request == id => Some(reason),
            _ => None,
        });
        assert_eq!(reason, "This session is still resuming");
        std::fs::remove_file(&sock).unwrap();
    }

    #[test]
    fn a_prompt_sent_while_pocketd_is_unreachable_joins_the_list_and_goes_out_once_it_is_back() {
        let sock = sock_path("down");
        let (out, events) = connect(&sock);
        let id = out.prompt("a1", "Fix it");
        let mut events = futures::executor::block_on_stream(events);
        let held = next_matching(&mut events, |ev| match ev {
            Event::PromptState { request, state, .. } => Some((request, state)),
            _ => None,
        });
        assert_eq!(held, (id.clone(), PromptState::Resending));

        let server = UnixListener::bind(&sock).unwrap();
        let mut ws = accept(&server);
        welcome(&mut ws, true);
        let sent = read(&mut ws);
        assert_eq!((sent["type"].as_str(), sent["id"].as_str()), (Some("agent.prompt"), Some(id.as_str())));
        std::fs::remove_file(&sock).unwrap();
    }

    #[test]
    fn a_prompt_failed_for_the_link_being_down_too_long_is_never_sent_once_pocketd_is_back() {
        let sock = sock_path("expired");
        let (tx, mut events) = unbounded();
        let (out, queue) = channel();
        let mut carry = Carry::new(String::new(), None);
        carry.resend = Resend::new(Instant::now() - DOWN_LIMIT);
        out.send(json!({"type": "agent.prompt", "id": "prompt-x-1", "agentId": "a1", "text": "Fix it"})).unwrap();
        carry.wait_out(&queue, &tx, Duration::from_millis(50));
        let failed = std::iter::from_fn(|| events.try_recv().ok()).find_map(|ev| match ev {
            Event::PromptFailed { request, .. } => Some(request),
            _ => None,
        });
        assert_eq!(failed.as_deref(), Some("prompt-x-1"));

        let server = UnixListener::bind(&sock).unwrap();
        let link = sock.clone();
        std::thread::spawn(move || run(&link, None, &tx, &queue, &mut carry, None));
        let mut ws = accept(&server);
        welcome(&mut ws, true);
        out.send(json!({"type": "agent.seen", "agentIds": ["a1"]})).unwrap();
        assert_eq!(read(&mut ws)["type"], "agent.seen", "the failed prompt stays unsent");
        std::fs::remove_file(&sock).unwrap();
    }

    #[test]
    fn what_else_is_sent_while_pocketd_is_unreachable_waits_for_it_in_order() {
        let sock = sock_path("heldorder");
        let (out, _events) = connect(&sock);
        out.view(&["a1".into()]);
        let id = out.prompt("a1", "Fix it");
        out.seen(&["a1".into()]);
        std::thread::sleep(Duration::from_millis(100));

        let server = UnixListener::bind(&sock).unwrap();
        let mut ws = accept(&server);
        assert_eq!(read(&mut ws)["type"], "hello");
        let order: Vec<String> = (0..3).map(|_| read(&mut ws)["type"].as_str().unwrap_or_default().to_string()).collect();
        assert_eq!(order, ["agent.view", "agent.prompt", "agent.seen"], "{id} between them");
        std::fs::remove_file(&sock).unwrap();
    }

    #[test]
    fn the_log_says_what_happened_to_a_prompt_and_never_what_it_said() {
        let dir = std::env::temp_dir().join(format!("link-log-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("logs/desktop.log");
        let (server, sock) = pocketd("logged");
        let (out, events) = connect_with(&sock, None, Peer::ThisMac, LinkLog::open(&path));
        let mut ws = accept(&server);
        welcome(&mut ws, true);
        let id = out.prompt("a1b2c3d4e5", "the secret plan");
        read(&mut ws);
        drop(ws);

        let mut ws = accept(&server);
        welcome(&mut ws, true);
        read(&mut ws);
        let failed = json!({"type": "agent.delivery", "id": id, "agentId": "a1b2c3d4e5", "state": "failed", "error": "This session is still resuming"});
        ws.send(Message::text(failed.to_string())).unwrap();
        let mut events = futures::executor::block_on_stream(events);
        next_matching(&mut events, |ev| matches!(ev, Event::PromptFailed { .. }).then_some(()));

        let log = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = log.lines().map(|l| l.splitn(3, ' ').nth(2).unwrap_or(l)).collect();
        let socket = sock.file_name().unwrap().to_string_lossy().into_owned();
        let expected = [
            "link up".to_string(),
            "link down".to_string(),
            "link up".to_string(),
            format!("resent prompt {id} to a1b2c3d4"),
            format!("prompt {id} to a1b2c3d4 not delivered: This session is still resuming"),
        ];
        assert_eq!(lines, expected.iter().map(|l| format!("{socket}: {l}")).collect::<Vec<_>>());
        assert!(!log.contains("secret plan"));
        std::fs::remove_dir_all(dir).unwrap();
        std::fs::remove_file(&sock).unwrap();
    }

    #[test]
    fn each_stop_goes_out_under_its_own_id_and_its_refusal_reaches_the_app() {
        let (server, sock) = pocketd("interrupt");
        let (out, events) = connect(&sock);
        let mut ws = accept(&server);
        read(&mut ws);
        out.interrupt("a1");
        out.interrupt("a1");
        let (first, second) = (read(&mut ws)["id"].clone(), read(&mut ws)["id"].clone());
        assert_ne!(first, second);
        let refusal = json!({"type": "error", "id": second, "message": "Agent is no longer in the foreground"});
        ws.send(Message::text(refusal.to_string())).unwrap();
        let mut events = futures::executor::block_on_stream(events);
        assert!(matches!(events.next(), Some(Event::InterruptFailed { agent, reason }) if agent == "a1" && reason == "Agent is no longer in the foreground"));
        std::fs::remove_file(&sock).unwrap();
    }

    #[test]
    fn rename_sends_the_path_and_title() {
        let (server, sock) = pocketd("rename");
        let (out, _events) = connect(&sock);
        let mut ws = accept(&server);
        read(&mut ws);
        out.rename("/w/calm-otter", "Fix login");
        assert_eq!(read(&mut ws), json!({"type": "worktree.rename", "id": "rename", "path": "/w/calm-otter", "title": "Fix login"}));
        std::fs::remove_file(&sock).unwrap();
    }

    #[test]
    fn local_messages_carry_the_local_id() {
        let (server, sock) = pocketd("locals");
        let (out, _events) = connect(&sock);
        let mut ws = accept(&server);
        read(&mut ws);
        out.local_create(&Local { id: "l1".into(), project: "/p".into(), name: "Local 2".into() });
        out.local_rename("l1", "Fix");
        out.local_delete("l1");
        assert_eq!(read(&mut ws), json!({"type": "local.create", "id": "local", "localId": "l1", "project": "/p", "name": "Local 2"}));
        assert_eq!(read(&mut ws), json!({"type": "local.rename", "id": "local", "localId": "l1", "title": "Fix"}));
        assert_eq!(read(&mut ws), json!({"type": "local.delete", "id": "local", "localId": "l1"}));
        std::fs::remove_file(&sock).unwrap();
    }
}
