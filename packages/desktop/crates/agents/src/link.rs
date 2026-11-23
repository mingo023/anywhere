use crate::event::{Frame, automation_event, board_event, delegation_event, host_event, launch_event, names_event};
use crate::ids::PAIR;
use crate::caps::CAPS;
use crate::{Event, Outbox, hosts, queue};
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use serde_json::{Value, json};
use std::collections::{HashMap, VecDeque};
use std::io::ErrorKind;
#[cfg(unix)]
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;
use tungstenite::Message;
#[cfg(windows)]
use uds_windows::UnixStream;

/// Why a prompt or stop the socket never took failed.
const UNSENT: &str = "Lost the connection to pocketd";
/// How long a pocketd may take to answer the websocket handshake, and to take each write after it, before the link counts as lost.
const STALL: Duration = Duration::from_secs(5);
const PAGE: usize = 500;

/// Follows pocketd's phone protocol over its unix socket, where pocketd tells
/// the owner from a process inside a Terminal: the agent list and every agent's timeline.
pub fn connect(sock: &Path) -> (Outbox, UnboundedReceiver<Event>) {
    connect_with(sock, None)
}

/// `connect` to a socket behind which pocketd wants an owner token: a Windows host's forwarded TCP port.
pub fn connect_with(sock: &Path, token: Option<String>) -> (Outbox, UnboundedReceiver<Event>) {
    let (tx, rx) = unbounded();
    let (out, queue) = channel();
    let sock = sock.to_path_buf();
    std::thread::spawn(move || {
        let mut unanswered = Vec::new();
        let mut requests = Requests::default();
        loop {
            let _ = run(&sock, token.as_deref(), &tx, &queue, &mut unanswered, &mut requests, STALL);
            for ev in requests.refuse_all(UNSENT) {
                let _ = tx.unbounded_send(ev);
            }
            if tx.is_closed() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_secs(2));
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

fn run(sock: &Path, token: Option<&str>, tx: &UnboundedSender<Event>, queue: &Receiver<Value>, unanswered: &mut Vec<Value>, requests: &mut Requests, stall: Duration) -> Option<()> {
    let stream = UnixStream::connect(sock).ok()?;
    stream.set_read_timeout(Some(stall)).ok()?;
    stream.set_write_timeout(Some(stall)).ok()?;
    let (mut ws, _) = tungstenite::client("ws://localhost/", stream).ok()?;
    // Reads time out so the loop gets to send the queue even while pocketd is quiet; the timeout is as long as a
    // send can wait, so it stays short.
    ws.get_ref().set_read_timeout(Some(Duration::from_millis(10))).ok()?;
    ws.send(Message::text(hello(token).to_string())).ok()?;
    for m in unanswered.iter() {
        ws.send(Message::text(m.to_string())).ok()?;
    }
    let mut known: Vec<String> = Vec::new();
    let mut asked_before: HashMap<String, i64> = HashMap::new();
    loop {
        for m in queue.try_iter() {
            if m["type"] == "agent.create" {
                unanswered.push(m.clone());
            }
            if ws.send(Message::text(m.to_string())).is_err() {
                if let Some(ev) = refused(&m, UNSENT) {
                    let _ = tx.unbounded_send(ev);
                }
                return None;
            }
            requests.sent(&m);
        }
        let raw = match ws.read() {
            Ok(Message::Text(raw)) => raw,
            Ok(_) => continue,
            Err(tungstenite::Error::Io(e)) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => continue,
            Err(_) => return None,
        };
        let Ok(f) = serde_json::from_str::<Frame>(&raw) else { continue };
        if !matches!(f.kind.as_str(), "agent.creating" | "agent.progress") {
            unanswered.retain(|m| m["id"] != f.id.as_str());
        }
        if let Some(ev) = requests.answer(&f).or_else(|| launch_event(&f)).or_else(|| names_event(&f)).or_else(|| automation_event(&f)).or_else(|| board_event(&f)).or_else(|| hosts::decode(&f.kind, &raw)).or_else(|| delegation_event(&f.kind, &raw)).or_else(|| queue::decode(&f.kind, &raw).map(|(agent, entries)| Event::Queue { agent, entries })) {
            tx.unbounded_send(ev).ok()?;
            continue;
        }
        let mut fresh: Vec<String> = Vec::new();
        let ev = match f.kind.as_str() {
            "hello.ok" => {
                let host = host_event(&f);
                tx.unbounded_send(Event::Connected { scopes: f.scopes, caps: f.caps, version: f.version }).ok()?;
                let Some(ev) = host else { continue };
                ev
            }
            "agent.list" => {
                fresh.extend(f.agents.iter().map(|a| a.id.clone()));
                Event::Agents(f.agents)
            }
            "agent.update" => {
                let Some(a) = f.agent else { continue };
                fresh.push(a.id.clone());
                Event::Agent(Box::new(a))
            }
            "agent.stream" => Event::Items(f.agent_id, f.item.into_iter().collect()),
            "agent.timeline" => {
                let asked = asked_before.remove(&f.agent_id);
                if let Some((before, req)) = older_request(&f, asked) {
                    ws.send(Message::text(req.to_string())).ok()?;
                    asked_before.insert(f.agent_id.clone(), before);
                }
                // An older page only adds: the held items it doesn't cover are newer, not stale.
                if asked.is_some() { Event::Items(f.agent_id, f.items) } else { Event::Page { agent: f.agent_id, items: f.items, max_seq: f.max_seq } }
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
        for id in fresh {
            if known.contains(&id) {
                continue;
            }
            let req = json!({"type": "agent.timeline", "id": format!("t-{id}"), "agentId": id, "limit": PAGE});
            ws.send(Message::text(req.to_string())).ok()?;
            known.push(id);
        }
    }
}

/// `asked` is the `beforeSeq` page `f` answers: a pocketd that ignored it sends nothing older, so it isn't asked again.
fn older_request(f: &Frame, asked: Option<i64>) -> Option<(i64, Value)> {
    let oldest = f.items.iter().map(|x| x.seq).min()?;
    (f.has_older && asked.is_none_or(|b| oldest < b))
        .then(|| (oldest, json!({"type": "agent.timeline", "id": format!("o-{}", f.agent_id), "agentId": f.agent_id, "beforeSeq": oldest, "limit": PAGE})))
}

/// Prompts, stops and queue changes written to pocketd and not yet answered, so a refusal names the one it refused.
#[derive(Default)]
struct Requests(VecDeque<Value>);

impl Requests {
    fn sent(&mut self, m: &Value) {
        if matches!(m["type"].as_str(), Some("agent.prompt" | "agent.interrupt" | "queue.edit" | "queue.cancel" | "queue.reorder" | "queue.send_now")) {
            self.0.push_back(m.clone());
        }
    }

    /// Drops the request `f` answers; a refusal becomes `PromptFailed` or `InterruptFailed`.
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
fn refused(m: &Value, reason: &str) -> Option<Event> {
    let field = |key: &str| m[key].as_str().unwrap_or_default().to_string();
    let queue_failed = |text: Option<String>| Some(Event::QueueFailed { agent: field("agentId"), reason: reason.into(), text });
    match m["type"].as_str()? {
        "agent.prompt" if m["queue"] == true => queue_failed(Some(field("text"))),
        "queue.edit" => queue_failed(Some(field("text"))),
        "queue.cancel" | "queue.reorder" | "queue.send_now" => queue_failed(None),
        "agent.prompt" => Some(Event::PromptFailed { agent: field("agentId"), request: field("id") }),
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
    #[cfg(unix)]
    use std::os::unix::net::UnixListener;
    #[cfg(windows)]
    use uds_windows::UnixListener;
    use std::path::PathBuf;

    type Ws = tungstenite::WebSocket<UnixStream>;

    fn pocketd(name: &str) -> (UnixListener, PathBuf) {
        let sock = PathBuf::from("/tmp").join(format!("pa-{name}-{}.sock", std::process::id()));
        let _ = std::fs::remove_file(&sock);
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
        std::thread::spawn(move || done_tx.send(run(&sock, None, &tx, &queue, &mut Vec::new(), &mut Requests::default(), Duration::from_millis(200))));
        assert_eq!(done.recv_timeout(Duration::from_secs(2)), Ok(None));
    }

    #[test]
    fn sends_queued_messages_while_pocketd_is_quiet() {
        let (server, sock) = pocketd("queue");
        let (out, _events) = connect(&sock);
        let mut ws = accept(&server);

        assert_eq!(
            read(&mut ws),
            json!({"type": "hello", "id": "h", "clientId": "desktop", "protocolVersion": 3, "caps": ["pair.v1", "scopes.v1", "summary.v2", "host.v1", "open.v1", "names.v1", "automations.v1", "board.v1", "locals.v1", "fork.v1", "accounts.v1", "subagents.v1", "settings.v1", "commands.v1", "access.v1", "skills.v1", "orchestrator.v1", "hosts.v1"]})
        );
        out.view(&["a1".into()]);
        assert_eq!(read(&mut ws), json!({"type": "agent.view", "id": "view", "agentIds": ["a1"]}));
        out.seen(&["a1".into()]);
        assert_eq!(read(&mut ws), json!({"type": "agent.seen", "id": "seen", "agentIds": ["a1"]}));
        std::fs::remove_file(&sock).unwrap();
    }

    fn page(seqs: std::ops::RangeInclusive<i64>, older: bool) -> Message {
        let items: Vec<Value> = seqs.map(|s| json!({"id": format!("i{s}"), "seq": s, "kind": "assistant"})).collect();
        Message::text(json!({"type": "agent.timeline", "agentId": "a1", "hasOlder": older, "maxSeq": 6, "items": items}).to_string())
    }

    #[test]
    fn a_chat_longer_than_a_page_is_paged_back_to_its_first_item() {
        let (server, sock) = pocketd("older");
        let (_out, events) = connect(&sock);
        let mut ws = accept(&server);
        read(&mut ws);
        ws.send(Message::text(json!({"type": "agent.list", "agents": [{"id": "a1"}]}).to_string())).unwrap();
        assert_eq!(read(&mut ws)["limit"], 500);
        ws.send(page(5..=6, true)).unwrap();
        assert_eq!(read(&mut ws), json!({"type": "agent.timeline", "id": "o-a1", "agentId": "a1", "beforeSeq": 5, "limit": 500}));
        ws.send(page(3..=4, true)).unwrap();
        assert_eq!(read(&mut ws)["beforeSeq"], 3);
        ws.send(page(1..=2, false)).unwrap();

        let mut a = Agents::default();
        for ev in futures::executor::block_on_stream(events).take(4) {
            a.apply(ev);
        }
        assert_eq!(a.timelines["a1"].iter().map(|i| i.seq).collect::<Vec<_>>(), [1, 2, 3, 4, 5, 6]);
        std::fs::remove_file(&sock).unwrap();
    }

    #[test]
    fn a_pocketd_that_ignores_before_seq_is_not_asked_again() {
        let (server, sock) = pocketd("ignores");
        let (_out, _events) = connect(&sock);
        let mut ws = accept(&server);
        read(&mut ws);
        ws.send(Message::text(json!({"type": "agent.list", "agents": [{"id": "a1"}]}).to_string())).unwrap();
        read(&mut ws);
        ws.send(page(5..=6, true)).unwrap();
        assert_eq!(read(&mut ws)["beforeSeq"], 5);
        ws.send(page(5..=6, true)).unwrap();
        ws.get_ref().set_read_timeout(Some(Duration::from_millis(500))).unwrap();
        assert!(ws.read().is_err(), "asked for the same page again");
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
    fn a_refused_prompt_reports_which_send_failed() {
        let mut prompts = Requests::default();
        prompts.sent(&json!({"type": "agent.prompt", "id": "prompt-1", "agentId": "a1", "text": "Fix it"}));
        prompts.sent(&json!({"type": "agent.prompt", "id": "prompt-2", "agentId": "a2", "text": "Ship it"}));
        let refused = prompts.answer(&frame(json!({"type": "error", "id": "prompt-2", "message": "Agent isn't in the foreground"})));
        assert!(matches!(refused, Some(Event::PromptFailed { agent, request }) if agent == "a2" && request == "prompt-2"));
        assert!(prompts.answer(&frame(json!({"type": "error", "id": "prompt-2", "message": "again"}))).is_none());
    }

    #[test]
    fn an_acked_prompt_reports_nothing() {
        let mut prompts = Requests::default();
        prompts.sent(&json!({"type": "agent.prompt", "id": "prompt-1", "agentId": "a1", "text": "Fix it"}));
        assert!(prompts.answer(&frame(json!({"type": "ack", "id": "prompt-1"}))).is_none());
        assert!(prompts.answer(&frame(json!({"type": "error", "id": "prompt-1", "message": "late"}))).is_none());
    }

    #[test]
    fn a_refused_stop_reports_its_agent_and_why() {
        let mut requests = Requests::default();
        requests.sent(&json!({"type": "agent.interrupt", "id": "interrupt-1", "agentId": "a1"}));
        requests.sent(&json!({"type": "agent.interrupt", "id": "interrupt-2", "agentId": "a2"}));
        assert!(requests.answer(&frame(json!({"type": "ack", "id": "interrupt-1"}))).is_none());
        let refused = requests.answer(&frame(json!({"type": "error", "id": "interrupt-2", "code": "agent_resuming", "message": "This session is still resuming"})));
        assert!(matches!(refused, Some(Event::InterruptFailed { agent, reason }) if agent == "a2" && reason == "This session is still resuming"));
    }

    #[test]
    fn a_refused_queue_change_or_queued_send_names_its_session_and_gives_back_the_text() {
        let (tx, rx) = std::sync::mpsc::channel();
        let out = Outbox::from(tx);
        out.queue_edit("a1", "e1", "fix lint too");
        out.queue_cancel("a1", "e2");
        out.queue_reorder("a1", "e3", "e1");
        out.queue_send_now("a1", "e4");
        let queued = out.prompt_queued("a2", "after this");
        let mut requests = Requests::default();
        let sent: Vec<Value> = rx.try_iter().collect();
        sent.iter().for_each(|m| requests.sent(m));
        let refuse = |requests: &mut Requests, m: &Value| requests.answer(&frame(json!({"type": "error", "id": m["id"], "message": "No such entry"})));
        let got: Vec<_> = sent.iter().map(|m| refuse(&mut requests, m)).collect();
        assert!(matches!(&got[0], Some(Event::QueueFailed { agent, reason, text }) if agent == "a1" && reason == "No such entry" && text.as_deref() == Some("fix lint too")));
        assert!(matches!(&got[1], Some(Event::QueueFailed { agent, text: None, .. }) if agent == "a1"));
        assert!(matches!(&got[2], Some(Event::QueueFailed { text: None, .. })));
        assert!(matches!(&got[3], Some(Event::QueueFailed { text: None, .. })));
        assert!(matches!(&got[4], Some(Event::QueueFailed { agent, text, .. }) if agent == "a2" && text.as_deref() == Some("after this")), "{queued} is a queued send, not a prompt of the transcript");
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
        assert!(matches!(refused(&unsent, UNSENT), Some(Event::PromptFailed { agent, request }) if agent == "a1" && request == "prompt-1"));
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
        assert!(matches!(events.next(), Some(Event::PromptFailed { agent, request }) if agent == "a1" && request == second));
        std::fs::remove_file(&sock).unwrap();
    }

    #[test]
    fn a_prompt_or_stop_still_unanswered_when_the_link_drops_fails() {
        let (server, sock) = pocketd("dropped");
        let (out, events) = connect(&sock);
        let mut ws = accept(&server);
        read(&mut ws);
        let prompt = out.prompt("a1", "Fix it");
        out.interrupt("a2");
        assert_eq!((read(&mut ws)["id"].as_str(), read(&mut ws)["type"].as_str()), (Some(prompt.as_str()), Some("agent.interrupt")));
        drop(ws);
        let (tx, failures) = std::sync::mpsc::channel();
        std::thread::spawn(move || futures::executor::block_on_stream(events).take(2).for_each(|ev| drop(tx.send(ev))));
        let next = || failures.recv_timeout(Duration::from_secs(2)).ok();
        assert!(matches!(next(), Some(Event::PromptFailed { agent, request }) if agent == "a1" && request == prompt));
        assert!(matches!(next(), Some(Event::InterruptFailed { agent, reason }) if agent == "a2" && reason == UNSENT));
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
