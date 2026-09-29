use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::ErrorKind;
use std::net::TcpStream;
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
}

pub enum Event {
    Agents(Vec<Summary>),
    Agent(Summary),
    Items(String, Vec<Item>),
    Asked(Permission),
    Resolved(String),
    Connected,
}

#[derive(Default)]
pub struct Agents {
    pub list: Vec<Summary>,
    pub timelines: HashMap<String, Vec<Item>>,
    pub pending: Vec<Permission>,
}

impl Agents {
    pub fn get(&self, id: &str) -> Option<&Summary> {
        self.list.iter().find(|a| a.id == id)
    }

    pub fn apply(&mut self, ev: Event) {
        match ev {
            Event::Agents(list) => self.list = list,
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
            Event::Connected => self.pending.clear(),
        }
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

fn token(home: &Path) -> Option<(String, u16)> {
    let raw = std::fs::read(home.join("config.json")).ok()?;
    let v: Value = serde_json::from_slice(&raw).ok()?;
    Some((v["token"].as_str()?.to_string(), v["port"].as_u64()? as u16))
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

    pub fn close(&self, id: &str) {
        self.send(json!({"type": "agent.close", "id": "close", "agentId": id}));
    }

    fn send(&self, m: Value) {
        let _ = self.0.send(m.to_string());
    }
}

/// Follows pocketd's phone protocol on localhost: the agent list and every agent's timeline.
pub fn connect(home: &Path) -> (Outbox, UnboundedReceiver<Event>) {
    let (tx, rx) = unbounded();
    let (out, queue) = channel();
    let home = home.to_path_buf();
    std::thread::spawn(move || {
        loop {
            let _ = run(&home, &tx, &queue);
            if tx.is_closed() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_secs(2));
        }
    });
    (Outbox(out), rx)
}

fn run(home: &Path, tx: &UnboundedSender<Event>, queue: &Receiver<String>) -> Option<()> {
    let (tok, port) = token(home)?;
    let stream = TcpStream::connect(("127.0.0.1", port)).ok()?;
    let (mut ws, _) = tungstenite::client(format!("ws://127.0.0.1:{port}"), stream).ok()?;
    // Reads time out so the loop gets to send the queue even while pocketd is quiet.
    ws.get_ref().set_read_timeout(Some(Duration::from_millis(100))).ok()?;
    let hello = json!({"type": "hello", "id": "h", "token": tok, "clientId": "desktop", "protocolVersion": 3});
    ws.send(Message::text(hello.to_string())).ok()?;
    tx.unbounded_send(Event::Connected).ok()?;
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
    use std::net::TcpListener;

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
        a.apply(Event::Connected);
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

    #[test]
    fn sends_queued_messages_while_pocketd_is_quiet() {
        let server = TcpListener::bind("127.0.0.1:0").unwrap();
        let home = std::env::temp_dir().join(format!("pocket-agents-{}", std::process::id()));
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(home.join("config.json"), json!({"token": "t", "port": server.local_addr().unwrap().port()}).to_string()).unwrap();
        let (out, _events) = connect(&home);
        let (peer, _) = server.accept().unwrap();
        peer.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut ws = tungstenite::accept(peer).unwrap();
        let read = |ws: &mut tungstenite::WebSocket<TcpStream>| serde_json::from_str::<Value>(ws.read().unwrap().to_text().unwrap()).unwrap();

        assert_eq!(read(&mut ws)["type"], "hello");
        out.view(&["a1".into()]);
        assert_eq!(read(&mut ws), json!({"type": "agent.view", "id": "view", "agentIds": ["a1"]}));
        out.seen(&["a1".into()]);
        assert_eq!(read(&mut ws), json!({"type": "agent.seen", "id": "seen", "agentIds": ["a1"]}));
        out.close("a1");
        assert_eq!(read(&mut ws), json!({"type": "agent.close", "id": "close", "agentId": "a1"}));
        std::fs::remove_dir_all(&home).unwrap();
    }
}
