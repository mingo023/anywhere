use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::ErrorKind;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Duration;
use tungstenite::Message;

#[derive(Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Summary {
    pub id: String,
    pub terminal_id: String,
    pub title: String,
    pub cwd: String,
    pub provider: String,
    pub model: Option<String>,
    pub status: String,
    pub failed: bool,
    pub attached: bool,
    pub compacting: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Item {
    pub id: String,
    pub seq: i64,
    pub ts: i64,
    pub kind: String,
    pub text: String,
    pub error: String,
    pub duration_ms: i64,
    pub usage: Option<Usage>,
    pub call: Option<Call>,
}

#[derive(Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Call {
    pub detail: Detail,
}

#[derive(Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Usage {
    pub input_tokens: u64,
    pub cache_read_tokens: u64,
}

/// Both providers' default models have a 200k-token window; pocketd doesn't report it.
const CONTEXT_WINDOW: u64 = 200_000;

#[derive(Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Detail {
    pub kind: String,
    pub command: String,
    pub path: String,
    pub query: String,
    pub description: String,
    pub name: String,
}

#[derive(Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Permission {
    pub request_id: String,
    pub agent_id: String,
    pub tool_name: String,
    pub detail: Detail,
}

impl Permission {
    pub fn ask(&self) -> String {
        let d = &self.detail;
        match d.kind.as_str() {
            "shell" => format!("Wants to run {}", d.command),
            "edit" | "write" | "read" => format!("Wants to {} {}", d.kind, d.path),
            "search" => format!("Wants to search {}", d.query),
            "task" => format!("Wants to start {}", d.description),
            _ => format!("Wants to use {}", if d.name.is_empty() { &self.tool_name } else { &d.name }),
        }
    }
}

#[derive(Deserialize, Default, Debug)]
#[serde(default, rename_all = "camelCase")]
struct Frame {
    #[serde(rename = "type")]
    kind: String,
    agent_id: String,
    agents: Vec<Summary>,
    agent: Option<Summary>,
    item: Option<Item>,
    items: Vec<Item>,
    request: Option<Permission>,
    request_id: String,
    scopes: Vec<String>,
}

pub enum Event {
    Agents(Vec<Summary>),
    Agent(Summary),
    Items(String, Vec<Item>),
    Asked(Permission),
    Resolved(String),
    /// hello.ok arrived; carries the scopes pocketd granted this conn.
    Connected(Vec<String>),
}

#[derive(Default)]
pub struct Agents {
    pub list: Vec<Summary>,
    pub timelines: HashMap<String, Vec<Item>>,
    pub pending: Vec<Permission>,
    pub scopes: Vec<String>,
}

impl Agents {
    pub fn get(&self, id: &str) -> Option<&Summary> {
        self.list.iter().find(|a| a.id == id)
    }

    pub fn apply(&mut self, ev: Event) {
        match ev {
            Event::Agents(list) => self.list = list,
            Event::Agent(a) if a.status == "closed" => self.list.retain(|x| x.id != a.id),
            Event::Agent(a) => match self.list.iter_mut().find(|x| x.id == a.id) {
                Some(x) => *x = a,
                None => self.list.push(a),
            },
            Event::Items(id, items) => {
                let t = self.timelines.entry(id).or_default();
                for item in items {
                    match t.iter_mut().find(|x| x.id == item.id) {
                        Some(x) => *x = item,
                        None => t.push(item),
                    }
                }
                t.sort_by_key(|x| x.seq);
            }
            Event::Asked(p) => {
                if !self.pending.iter().any(|x| x.request_id == p.request_id) {
                    self.pending.push(p);
                }
            }
            Event::Resolved(id) => self.pending.retain(|p| p.request_id != id),
            Event::Connected(scopes) => {
                self.pending.clear();
                self.scopes = scopes;
            }
        }
    }

    pub fn owner(&self) -> bool {
        self.scopes.iter().any(|s| s == "owner")
    }

    /// Scopes arrived and exclude owner. Before hello.ok, or from a pocketd
    /// that predates scopes, nothing is known, so nothing is disabled.
    pub fn observe_only(&self) -> bool {
        !self.scopes.is_empty() && !self.owner()
    }

    pub fn last_result(&self, id: &str) -> Option<&Item> {
        self.timelines.get(id)?.iter().rev().find(|i| i.kind == "result")
    }

    pub fn context_left(&self, id: &str) -> Option<u64> {
        let u = self.timelines.get(id)?.iter().rev().find_map(|i| i.usage.as_ref())?;
        Some(100 - ((u.input_tokens + u.cache_read_tokens) * 100 / CONTEXT_WINDOW).min(100))
    }

    /// The agent whose timeline last edited or wrote `path`, and when; tool paths may be absolute or relative to its cwd.
    pub fn last_edit(&self, path: &str) -> Option<(&Summary, i64)> {
        let touches = |d: &Detail, cwd: &str| {
            (d.kind == "edit" || d.kind == "write") && !d.path.is_empty() && (d.path == path || format!("{cwd}/{}", d.path) == path)
        };
        self.list
            .iter()
            .filter_map(|a| {
                let t = self.timelines.get(&a.id)?;
                let ts = t.iter().rev().find(|i| i.call.as_ref().is_some_and(|c| touches(&c.detail, &a.cwd)))?.ts;
                Some((a, ts))
            })
            .max_by_key(|(_, ts)| *ts)
    }

    pub fn last_text(&self, id: &str) -> Option<&str> {
        self.timelines.get(id)?.iter().rev().find(|i| i.kind == "assistant").map(|i| i.text.as_str())
    }
}

/// "claude-opus-5-5" reads as "Opus 5.5"; a missing model falls back to the provider.
pub fn model_label(a: &Summary) -> String {
    let Some(model) = a.model.as_deref().filter(|m| !m.is_empty()) else {
        let mut p = a.provider.chars();
        return p.next().map(|c| c.to_uppercase().chain(p).collect()).unwrap_or_default();
    };
    let mut words = model.strip_prefix("claude-").unwrap_or(model).split('-').filter(|w| w.len() < 8);
    let family = words.next().unwrap_or_default();
    let mut f = family.chars();
    let family: String = f.next().map(|c| c.to_uppercase().chain(f).collect()).unwrap_or_default();
    let version: Vec<&str> = words.collect();
    if version.is_empty() { family } else { format!("{family} {}", version.join(".")) }
}

/// A reply to a permission ask. pocketd interrupts the turn on a deny.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Decision {
    Allow,
    Deny,
}

/// Client messages for pocketd. They wait in a queue while it is unreachable.
#[derive(Clone)]
pub struct Outbox(Sender<String>);

impl Outbox {
    pub fn view(&self, ids: &[String]) {
        self.send(json!({"type": "agent.view", "id": "view", "agentIds": ids}));
    }

    pub fn seen(&self, ids: &[String]) {
        self.send(json!({"type": "agent.seen", "id": "seen", "agentIds": ids}));
    }

    pub fn resolve(&self, request_id: &str, decision: Decision) {
        let decision = match decision {
            Decision::Allow => "allow",
            Decision::Deny => "deny",
        };
        self.send(json!({"type": "permission.resolve", "id": "resolve", "requestId": request_id, "decision": decision}));
    }

    fn send(&self, m: Value) {
        let _ = self.0.send(m.to_string());
    }
}

/// Follows pocketd's phone protocol over its unix socket, where pocketd tells
/// the owner from a process inside a Terminal: the agent list and every agent's timeline.
pub fn connect(sock: &Path) -> (Outbox, UnboundedReceiver<Event>) {
    let (tx, rx) = unbounded();
    let (out, queue) = channel();
    let sock = sock.to_path_buf();
    std::thread::spawn(move || {
        loop {
            let _ = run(&sock, &tx, &queue);
            if tx.is_closed() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_secs(2));
        }
    });
    (Outbox(out), rx)
}

fn run(sock: &Path, tx: &UnboundedSender<Event>, queue: &Receiver<String>) -> Option<()> {
    let stream = UnixStream::connect(sock).ok()?;
    let (mut ws, _) = tungstenite::client("ws://localhost/", stream).ok()?;
    // Reads time out so the loop gets to send the queue even while pocketd is quiet.
    ws.get_ref().set_read_timeout(Some(Duration::from_millis(100))).ok()?;
    let hello = json!({"type": "hello", "id": "h", "clientId": "desktop", "protocolVersion": 3, "caps": ["pair.v1", "scopes.v1"]});
    ws.send(Message::text(hello.to_string())).ok()?;
    let mut known: Vec<String> = Vec::new();
    loop {
        for m in queue.try_iter() {
            ws.send(Message::text(m)).ok()?;
        }
        let raw = match ws.read() {
            Ok(Message::Text(raw)) => raw,
            Ok(_) => continue,
            Err(tungstenite::Error::Io(e)) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => continue,
            Err(_) => return None,
        };
        let Ok(f) = serde_json::from_str::<Frame>(&raw) else { continue };
        let mut fresh: Vec<String> = Vec::new();
        let ev = match f.kind.as_str() {
            "hello.ok" => Event::Connected(f.scopes),
            "agent.list" => {
                fresh.extend(f.agents.iter().map(|a| a.id.clone()));
                Event::Agents(f.agents)
            }
            "agent.update" => {
                let Some(a) = f.agent else { continue };
                fresh.push(a.id.clone());
                Event::Agent(a)
            }
            "agent.stream" => Event::Items(f.agent_id, f.item.into_iter().collect()),
            "agent.timeline" => Event::Items(f.agent_id, f.items),
            "permission.request" => {
                let Some(r) = f.request else { continue };
                Event::Asked(r)
            }
            "permission.resolved" => Event::Resolved(f.request_id),
            _ => continue,
        };
        tx.unbounded_send(ev).ok()?;
        for id in fresh {
            if known.contains(&id) {
                continue;
            }
            let req = json!({"type": "agent.timeline", "id": format!("t-{id}"), "agentId": id, "limit": 500});
            ws.send(Message::text(req.to_string())).ok()?;
            known.push(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;
    use std::path::PathBuf;

    fn summary(provider: &str, model: Option<&str>) -> Summary {
        Summary { provider: provider.into(), model: model.map(Into::into), ..Default::default() }
    }

    #[test]
    fn labels_models_like_the_picker() {
        assert_eq!(model_label(&summary("claude", Some("claude-opus-5"))), "Opus 5");
        assert_eq!(model_label(&summary("claude", Some("claude-opus-5-5"))), "Opus 5.5");
        assert_eq!(model_label(&summary("claude", Some("claude-haiku-4-5-20251001"))), "Haiku 4.5");
        assert_eq!(model_label(&summary("codex", None)), "Codex");
    }

    #[test]
    fn an_exited_agent_leaves_the_list() {
        let mut a = Agents::default();
        let agent = |id: &str, status: &str| Summary { id: id.into(), status: status.into(), ..Default::default() };
        a.apply(Event::Agents(vec![agent("a", "idle"), agent("b", "working")]));
        a.apply(Event::Agent(agent("a", "closed")));
        a.apply(Event::Agent(agent("c", "closed")));
        assert_eq!(a.list, vec![agent("b", "working")]);
    }

    #[test]
    fn stream_items_upsert_in_seq_order() {
        let mut a = Agents::default();
        let item = |id: &str, seq, text: &str| Item { id: id.into(), seq, kind: "assistant".into(), text: text.into(), ..Default::default() };
        a.apply(Event::Items("a".into(), vec![item("2", 2, "b"), item("1", 1, "a")]));
        a.apply(Event::Items("a".into(), vec![item("2", 2, "b!")]));
        let t: Vec<_> = a.timelines["a"].iter().map(|i| i.text.as_str()).collect();
        assert_eq!(t, vec!["a", "b!"]);
    }

    #[test]
    fn permissions_stay_pending_until_resolved_or_reconnected() {
        let f: Frame = serde_json::from_str(r#"{"type":"permission.request","request":{"requestId":"r1","agentId":"a","toolName":"Bash","detail":{"kind":"shell","command":"pnpm test"}}}"#).unwrap();
        let p = f.request.unwrap();
        assert_eq!(p.ask(), "Wants to run pnpm test");
        let mut a = Agents::default();
        a.apply(Event::Asked(p.clone()));
        a.apply(Event::Asked(p));
        assert_eq!(a.pending.len(), 1);
        a.apply(Event::Resolved("r1".into()));
        assert!(a.pending.is_empty());
        a.apply(Event::Asked(Permission { request_id: "r2".into(), ..Default::default() }));
        a.apply(Event::Connected(vec![]));
        assert!(a.pending.is_empty());
    }

    #[test]
    fn context_left_uses_the_latest_reported_usage() {
        let mut a = Agents::default();
        let turn = |n| Item { kind: "result".into(), usage: Some(Usage { input_tokens: n, cache_read_tokens: 20_000 }), ..Default::default() };
        a.timelines.insert("a".into(), vec![turn(1_000), turn(56_000), Item::default()]);
        assert_eq!(a.context_left("a"), Some(62));
        assert_eq!(a.context_left("b"), None);
    }

    #[test]
    fn decodes_a_v3_summary() {
        let f: Frame = serde_json::from_str(include_str!("../../../../pocketd/internal/proto/testdata/golden/server/agent_update_failed.json")).unwrap();
        let a = f.agent.unwrap();
        assert_eq!((a.terminal_id.as_str(), a.status.as_str(), a.failed, a.attached, a.compacting), ("t1", "done", true, true, false));
    }

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

    #[test]
    fn sends_queued_messages_while_pocketd_is_quiet() {
        let (server, sock) = pocketd("queue");
        let (out, _events) = connect(&sock);
        let mut ws = accept(&server);

        assert_eq!(read(&mut ws), json!({"type": "hello", "id": "h", "clientId": "desktop", "protocolVersion": 3, "caps": ["pair.v1", "scopes.v1"]}));
        out.view(&["a1".into()]);
        assert_eq!(read(&mut ws), json!({"type": "agent.view", "id": "view", "agentIds": ["a1"]}));
        out.seen(&["a1".into()]);
        assert_eq!(read(&mut ws), json!({"type": "agent.seen", "id": "seen", "agentIds": ["a1"]}));
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
        assert!(a.owner() && !a.observe_only());
        a.apply(Event::Connected(vec!["observe".into()]));
        assert!(a.observe_only());
        std::fs::remove_file(&sock).unwrap();
    }

    #[test]
    fn resolve_sends_permission_resolve_with_the_decision() {
        let (tx, rx) = channel();
        let out = Outbox(tx);
        out.resolve("q1", Decision::Allow);
        out.resolve("q2", Decision::Deny);
        let sent: Vec<Value> = rx.try_iter().map(|m| serde_json::from_str(&m).unwrap()).collect();
        assert_eq!(
            sent,
            vec![
                json!({"type": "permission.resolve", "id": "resolve", "requestId": "q1", "decision": "allow"}),
                json!({"type": "permission.resolve", "id": "resolve", "requestId": "q2", "decision": "deny"}),
            ]
        );
    }
}
