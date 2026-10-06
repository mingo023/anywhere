use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::ErrorKind;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
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
    pub effort: Option<String>,
    pub status: String,
    pub failed: bool,
    pub attached: bool,
    pub restore: String,
    pub compacting: bool,
    pub pinned: bool,
    pub provider_session_id: Option<String>,
    pub tokens_used: u64,
    pub context_window: Option<u64>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Summary {
    /// Tokens used and the model's window, once pocketd reports a non-zero window.
    pub fn context(&self) -> Option<(u64, u64)> {
        self.context_window.filter(|&w| w > 0).map(|w| (self.tokens_used, w))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Level {
    Low,
    Warn,
    Danger,
}

/// The whole percent of `window` that `used` fills, capped at 100; an empty window reads as 0.
pub fn percent(used: u64, window: u64) -> u64 {
    used.saturating_mul(100).checked_div(window).unwrap_or(0).min(100)
}

/// Judged on the whole percent, so 74.99% is still Low.
pub fn level(used: u64, window: u64) -> Level {
    match percent(used, window) {
        0..75 => Level::Low,
        75..90 => Level::Warn,
        _ => Level::Danger,
    }
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
    host: Option<Host>,
    scopes: Vec<String>,
    caps: Vec<String>,
    id: String,
    message: String,
    url: String,
    code: String,
    expires_at: i64,
    name: String,
    terminal_id: String,
    cwd: String,
    setup: bool,
    detail: String,
    phone_max_access: String,
    step: String,
    note: String,
    names: HashMap<String, String>,
}

/// What pocketd reports about the Mac it runs on.
#[derive(Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Host {
    pub tailnet: bool,
    pub keeping_awake: bool,
}

/// The host state a `hello.ok` or `host.changed` frame carries.
fn host_event(f: &Frame) -> Option<Event> {
    matches!(f.kind.as_str(), "hello.ok" | "host.changed").then(|| f.host.clone().map(Event::Host)).flatten()
}

pub enum Event {
    Agents(Vec<Summary>),
    Agent(Summary),
    Items(String, Vec<Item>),
    Asked(Permission),
    Resolved(String),
    /// hello.ok arrived with the scopes pocketd granted this conn and the caps both sides speak.
    Connected { scopes: Vec<String>, caps: Vec<String> },
    Host(Host),
    /// A pairing code from `Outbox::pair_begin`; `expires_at` is Unix ms.
    PairCode { url: String, code: String, expires_at: i64 },
    /// The phone redeemed the code; carries the device's name.
    Paired(String),
    PairFailed(String),
    /// pocketd made the Terminal for a create; `setup` runs before the agent.
    Creating { request: String, terminal: String, cwd: String, setup: bool },
    /// The step a create has reached; a step sent again carries a note on how it went.
    Progress { request: String, step: String, note: String },
    Created { request: String, agent: String },
    CreateFailed { request: String, code: String, message: String, detail: String },
    Providers { phone_max: String },
    ConfigFailed(String),
    /// Every worktree's display name by path, sent whole.
    Names(HashMap<String, String>),
    /// pocketd couldn't name the session from its prompt.
    NamingFailed(String),
}

#[derive(Default)]
pub struct Agents {
    pub list: Vec<Summary>,
    pub timelines: HashMap<String, Vec<Item>>,
    pub pending: Vec<Permission>,
    /// `None` until pocketd reports it, and again after each reconnect.
    pub host: Option<Host>,
    pub scopes: Vec<String>,
    /// The most a paired phone may start a session with, from `agent.providers`.
    pub phone_max: String,
    pub caps: Vec<String>,
    /// Worktree display names by path, from `worktree.names`.
    pub names: HashMap<String, String>,
}

impl Agents {
    pub fn get(&self, id: &str) -> Option<&Summary> {
        self.list.iter().find(|a| a.id == id)
    }

    pub fn in_terminal(&self, terminal: &str) -> Option<&Summary> {
        self.list.iter().find(|a| a.terminal_id == terminal)
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
            Event::Connected { scopes, caps } => {
                self.pending.clear();
                self.host = None;
                self.scopes = scopes;
                self.caps = caps;
                self.names.clear();
            }
            Event::Host(h) => self.host = Some(h),
            Event::PairCode { .. } | Event::Paired(_) | Event::PairFailed(_) => {}
            Event::Providers { phone_max } => self.phone_max = phone_max,
            Event::Creating { .. } | Event::Progress { .. } | Event::Created { .. } | Event::CreateFailed { .. } | Event::ConfigFailed(_) | Event::NamingFailed(_) => {}
            Event::Names(names) => self.names = names,
        }
    }

    pub fn owner(&self) -> bool {
        self.scopes.iter().any(|s| s == "owner")
    }

    pub fn pairing(&self) -> bool {
        self.caps.iter().any(|c| c == PAIR_CAP)
    }

    /// pocketd can open an existing branch as a worktree.
    pub fn opens(&self) -> bool {
        self.caps.iter().any(|c| c == OPEN_CAP)
    }

    /// pocketd names worktrees from their prompt and takes renames.
    pub fn names_offered(&self) -> bool {
        self.caps.iter().any(|c| c == NAMES_CAP)
    }

    /// Shows a rename before pocketd echoes it; a blank title clears the name.
    pub fn set_name(&mut self, path: &str, title: &str) {
        match title.trim() {
            "" => self.names.remove(path),
            t => self.names.insert(path.to_string(), t.to_string()),
        };
    }

    /// Scopes arrived and exclude owner. Before hello.ok, or from a pocketd
    /// that predates scopes, nothing is known, so nothing is disabled.
    pub fn observe_only(&self) -> bool {
        !self.scopes.is_empty() && !self.owner()
    }

    pub fn last_result(&self, id: &str) -> Option<&Item> {
        self.timelines.get(id)?.iter().rev().find(|i| i.kind == "result")
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

const PAIR: &str = "pair";
const CREATE: &str = "create-";
const CONFIG: &str = "config";

static NEXT: AtomicU64 = AtomicU64::new(0);

fn launch_event(f: &Frame) -> Option<Event> {
    Some(match f.kind.as_str() {
        "agent.creating" => Event::Creating { request: f.request_id.clone(), terminal: f.terminal_id.clone(), cwd: f.cwd.clone(), setup: f.setup },
        "agent.progress" => Event::Progress { request: f.request_id.clone(), step: f.step.clone(), note: f.note.clone() },
        "agent.created" => Event::Created { request: f.request_id.clone(), agent: f.agent_id.clone() },
        "agent.providers" => Event::Providers { phone_max: f.phone_max_access.clone() },
        "error" if f.id == CONFIG => Event::ConfigFailed(f.message.clone()),
        "error" => Event::CreateFailed { request: f.id.strip_prefix(CREATE)?.to_string(), code: f.code.clone(), message: f.message.clone(), detail: f.detail.clone() },
        _ => return None,
    })
}

fn names_event(f: &Frame) -> Option<Event> {
    match f.kind.as_str() {
        "worktree.names" => Some(Event::Names(f.names.clone())),
        "naming.failed" => Some(Event::NamingFailed(f.agent_id.clone())),
        _ => None,
    }
}
const PAIR_CAP: &str = "pair.v1";
const OPEN_CAP: &str = "open.v1";
const NAMES_CAP: &str = "names.v1";

/// Client messages for pocketd. They wait in a queue while it is unreachable.
#[derive(Clone)]
pub struct Outbox(Sender<Value>);

impl Outbox {
    pub fn view(&self, ids: &[String]) {
        self.send(json!({"type": "agent.view", "id": "view", "agentIds": ids}));
    }

    pub fn seen(&self, ids: &[String]) {
        self.send(json!({"type": "agent.seen", "id": "seen", "agentIds": ids}));
    }

    pub fn pin(&self, agent_id: &str, pinned: bool) {
        self.send(json!({"type": "agent.pin", "id": "pin", "agentId": agent_id, "pinned": pinned}));
    }

    pub fn resolve(&self, request_id: &str, decision: Decision) {
        let decision = match decision {
            Decision::Allow => "allow",
            Decision::Deny => "deny",
        };
        self.send(json!({"type": "permission.resolve", "id": "resolve", "requestId": request_id, "decision": decision}));
    }

    pub fn pair_begin(&self) {
        self.send(json!({"type": "pair.begin", "id": PAIR}));
    }

    /**
     * Sends `agent.create` and returns its requestId. It starts with the clock because pocketd
     * replays the receipt of a requestId it has seen, even from before a restart. Each reconnect
     * resends it under the same requestId until its final reply, which joins the first create.
     */
    pub fn create(&self, spec: Value) -> String {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());
        let request = format!("{nanos:x}-{}", NEXT.fetch_add(1, Ordering::Relaxed));
        self.send(json!({"type": "agent.create", "id": format!("{CREATE}{request}"), "requestId": request, "spec": spec}));
        request
    }

    pub fn providers(&self) {
        self.send(json!({"type": "agent.providers", "id": "providers"}));
    }

    pub fn set_phone_access(&self, access: &str) {
        self.send(json!({"type": "config.set", "id": CONFIG, "key": "phone.maxAccess", "value": access}));
        self.providers();
    }

    /// Sets the display name of the worktree at `path`; "" clears it.
    pub fn rename(&self, path: &str, title: &str) {
        self.send(json!({"type": "worktree.rename", "id": "rename", "path": path, "title": title}));
    }

    fn send(&self, m: Value) {
        let _ = self.0.send(m);
    }
}

/// Follows pocketd's phone protocol over its unix socket, where pocketd tells
/// the owner from a process inside a Terminal: the agent list and every agent's timeline.
pub fn connect(sock: &Path) -> (Outbox, UnboundedReceiver<Event>) {
    let (tx, rx) = unbounded();
    let (out, queue) = channel();
    let sock = sock.to_path_buf();
    std::thread::spawn(move || {
        let mut unanswered = Vec::new();
        loop {
            let _ = run(&sock, &tx, &queue, &mut unanswered);
            if tx.is_closed() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_secs(2));
        }
    });
    (Outbox(out), rx)
}

/// What this client understands beyond protocol 3.
const CAPS: [&str; 6] = [PAIR_CAP, "scopes.v1", "summary.v2", "host.v1", OPEN_CAP, NAMES_CAP];

fn run(sock: &Path, tx: &UnboundedSender<Event>, queue: &Receiver<Value>, unanswered: &mut Vec<Value>) -> Option<()> {
    let stream = UnixStream::connect(sock).ok()?;
    let (mut ws, _) = tungstenite::client("ws://localhost/", stream).ok()?;
    // Reads time out so the loop gets to send the queue even while pocketd is quiet.
    ws.get_ref().set_read_timeout(Some(Duration::from_millis(100))).ok()?;
    let hello = json!({"type": "hello", "id": "h", "clientId": "desktop", "protocolVersion": 3, "caps": CAPS});
    ws.send(Message::text(hello.to_string())).ok()?;
    for m in unanswered.iter() {
        ws.send(Message::text(m.to_string())).ok()?;
    }
    let mut known: Vec<String> = Vec::new();
    loop {
        for m in queue.try_iter() {
            if m["type"] == "agent.create" {
                unanswered.push(m.clone());
            }
            ws.send(Message::text(m.to_string())).ok()?;
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
        if let Some(ev) = launch_event(&f).or_else(|| names_event(&f)) {
            tx.unbounded_send(ev).ok()?;
            continue;
        }
        let mut fresh: Vec<String> = Vec::new();
        let ev = match f.kind.as_str() {
            "hello.ok" => {
                let host = host_event(&f);
                tx.unbounded_send(Event::Connected { scopes: f.scopes, caps: f.caps }).ok()?;
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
                Event::Agent(a)
            }
            "agent.stream" => Event::Items(f.agent_id, f.item.into_iter().collect()),
            "agent.timeline" => Event::Items(f.agent_id, f.items),
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
    fn an_agent_is_found_by_the_terminal_it_runs_in() {
        let agent = |id: &str, terminal: &str| Summary { id: id.into(), terminal_id: terminal.into(), ..Default::default() };
        let a = Agents { list: vec![agent("a", "t1"), agent("b", "t2")], ..Default::default() };
        assert_eq!(a.in_terminal("t2"), Some(&agent("b", "t2")));
        assert_eq!(a.in_terminal("shell"), None);
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
        a.apply(Event::Connected { scopes: vec![], caps: vec![] });
        assert!(a.pending.is_empty());
    }

    #[test]
    fn decodes_a_v3_summary() {
        let f: Frame = serde_json::from_str(include_str!("../../../../pocketd/internal/proto/testdata/golden/server/agent_update_failed.json")).unwrap();
        let a = f.agent.unwrap();
        assert_eq!((a.terminal_id.as_str(), a.status.as_str(), a.failed, a.attached, a.compacting), ("t1", "done", true, true, false));
    }

    #[test]
    fn summary_reads_the_provider_session_id() {
        let a: Summary = serde_json::from_str(r#"{"id":"a","providerSessionId":"s-1"}"#).unwrap();
        assert_eq!(a.provider_session_id.as_deref(), Some("s-1"));
        let fresh: Summary = serde_json::from_str(r#"{"id":"a"}"#).unwrap();
        assert_eq!(fresh.provider_session_id, None);
    }

    #[test]
    fn level_is_low_at_74_warn_at_75_and_89_danger_at_90() {
        let at = |used| level(used, 200_000);
        assert_eq!([at(148_000), at(149_998), at(150_000), at(179_998), at(180_000), at(400_000)], [Level::Low, Level::Low, Level::Warn, Level::Warn, Level::Danger, Level::Danger]);
    }

    #[test]
    fn an_empty_window_is_zero_percent_full() {
        assert_eq!((percent(184_000, 0), level(184_000, 0)), (0, Level::Low));
    }

    #[test]
    fn context_is_unknown_without_a_window() {
        let a = |window| Summary { tokens_used: 184_000, context_window: window, ..Default::default() };
        assert_eq!(a(None).context(), None);
        assert_eq!(a(Some(0)).context(), None);
        assert_eq!(a(Some(200_000)).context(), Some((184_000, 200_000)));
    }

    #[test]
    fn summary_reads_tokens_used_and_context_window() {
        let a: Summary = serde_json::from_str(r#"{"id":"a","tokensUsed":184000,"contextWindow":200000}"#).unwrap();
        assert_eq!((a.tokens_used, a.context_window), (184_000, Some(200_000)));
    }

    #[test]
    fn a_summary_without_restore_decodes() {
        let f: Frame = serde_json::from_str(include_str!("../../../../pocketd/internal/proto/testdata/golden/server/agent_update_restore_cleared.json")).unwrap();
        assert_eq!(f.agent.unwrap().restore, "");
        let a: Summary = serde_json::from_value(json!({"id": "a1", "restore": "interrupted"})).unwrap();
        assert_eq!(a.restore, "interrupted");
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

        assert_eq!(
            read(&mut ws),
            json!({"type": "hello", "id": "h", "clientId": "desktop", "protocolVersion": 3, "caps": ["pair.v1", "scopes.v1", "summary.v2", "host.v1", "open.v1", "names.v1"]})
        );
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
        assert!(a.owner() && !a.observe_only() && a.pairing());
        a.apply(Event::Connected { scopes: vec!["observe".into()], caps: vec![] });
        assert!(a.observe_only() && !a.pairing());
        std::fs::remove_file(&sock).unwrap();
    }

    #[test]
    fn opening_branches_waits_for_pocketd_to_offer_it() {
        let mut a = Agents::default();
        assert!(!a.opens());
        a.apply(Event::Connected { scopes: vec![], caps: vec!["open.v1".into()] });
        assert!(a.opens());
    }

    #[test]
    fn hello_ok_host_and_host_changed_become_host_events() {
        let mut a = Agents::default();
        let frame = |raw: &str| serde_json::from_str::<Frame>(raw).unwrap();
        a.apply(host_event(&frame(r#"{"type":"hello.ok","host":{"tailnet":false,"keepingAwake":true}}"#)).unwrap());
        assert_eq!(a.host, Some(Host { tailnet: false, keeping_awake: true }));
        a.apply(host_event(&frame(r#"{"type":"host.changed","host":{"tailnet":true,"keepingAwake":false}}"#)).unwrap());
        assert_eq!(a.host, Some(Host { tailnet: true, keeping_awake: false }));
        assert!(host_event(&frame(r#"{"type":"hello.ok"}"#)).is_none());
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
        assert_eq!(a.host, Some(Host { tailnet: true, keeping_awake: false }));
        ws.send(Message::text(changed)).unwrap();
        a.apply(got.next().unwrap());
        assert_eq!(a.host, Some(Host { tailnet: true, keeping_awake: true }));
        std::fs::remove_file(&sock).unwrap();
    }

    #[test]
    fn connecting_clears_the_host() {
        let mut a = Agents { host: Some(Host::default()), ..Default::default() };
        a.apply(Event::Connected { scopes: vec![], caps: vec![] });
        assert_eq!(a.host, None);
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
    fn resolve_sends_permission_resolve_with_the_decision() {
        let (tx, rx) = channel();
        let out = Outbox(tx);
        out.resolve("q1", Decision::Allow);
        out.resolve("q2", Decision::Deny);
        let sent: Vec<Value> = rx.try_iter().collect();
        assert_eq!(
            sent,
            vec![
                json!({"type": "permission.resolve", "id": "resolve", "requestId": "q1", "decision": "allow"}),
                json!({"type": "permission.resolve", "id": "resolve", "requestId": "q2", "decision": "deny"}),
            ]
        );
    }

    #[test]
    fn launch_replies_become_launch_events() {
        let ev = |raw: &str| launch_event(&serde_json::from_str::<Frame>(raw).unwrap());
        let creating = include_str!("../../../../pocketd/internal/proto/testdata/golden/server/agent_creating.json");
        let progress = include_str!("../../../../pocketd/internal/proto/testdata/golden/server/agent_progress.json");
        let created = include_str!("../../../../pocketd/internal/proto/testdata/golden/server/agent_created.json");
        let providers = include_str!("../../../../pocketd/internal/proto/testdata/golden/server/agent_providers.json");
        let failed = include_str!("../../../../pocketd/internal/proto/testdata/golden/server/error_detail.json");
        assert!(matches!(ev(creating), Some(Event::Creating { request, terminal, cwd, setup: true }) if request == "r1" && terminal == "t1" && cwd == "/w/fix"));
        assert!(matches!(ev(progress), Some(Event::Progress { request, step, note })
            if request == "r1" && step == "fetch" && note == "Couldn't fetch, using local main"));
        assert!(matches!(ev(created), Some(Event::Created { request, agent }) if request == "r1" && agent == "a1"));
        assert!(matches!(ev(providers), Some(Event::Providers { phone_max }) if phone_max == "ask"));
        assert!(matches!(ev(failed), Some(Event::CreateFailed { request, code, message, detail })
            if request == "r1" && code == "spawn_failed" && message == "Setup exited 1" && detail == "npm ERR! missing script: setup"));
        assert!(ev(r#"{"type":"error","id":"pair","message":"x"}"#).is_none());
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
        out.set_phone_access("auto");
        assert_eq!(read(&mut ws), json!({"type": "config.set", "id": "config", "key": "phone.maxAccess", "value": "auto"}));
        assert_eq!(read(&mut ws), json!({"type": "agent.providers", "id": "providers"}));
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
    fn a_refused_config_set_becomes_config_failed() {
        let f: Frame = serde_json::from_str(r#"{"type":"error","id":"config","code":"invalid_config","message":"phone.maxAccess can't be full"}"#).unwrap();
        assert!(matches!(launch_event(&f), Some(Event::ConfigFailed(m)) if m == "phone.maxAccess can't be full"));
    }

    #[test]
    fn names_frames_become_names_events() {
        let ev = |raw: &str| names_event(&serde_json::from_str::<Frame>(raw).unwrap());
        let names = include_str!("../../../../pocketd/internal/proto/testdata/golden/server/worktree_names.json");
        let failed = include_str!("../../../../pocketd/internal/proto/testdata/golden/server/naming_failed.json");
        assert!(matches!(ev(names), Some(Event::Names(n)) if n.get("/Users/me/wt/calm-otter").map(String::as_str) == Some("Fix login")));
        assert!(matches!(ev(failed), Some(Event::NamingFailed(id)) if id == "a1"));
        assert!(ev(r#"{"type":"agent.list","agents":[]}"#).is_none());
    }

    #[test]
    fn names_start_over_on_each_connect_and_wait_for_pocketd_to_offer_them() {
        let mut a = Agents::default();
        a.apply(Event::Names([("/w".to_string(), "Fix".to_string())].into()));
        assert!(!a.names_offered());
        a.apply(Event::Connected { scopes: vec![], caps: vec!["names.v1".into()] });
        assert!(a.names.is_empty() && a.names_offered());
    }

    #[test]
    fn a_rename_shows_at_once_and_a_blank_one_clears_it() {
        let mut a = Agents::default();
        a.set_name("/w", " Mine ");
        assert_eq!(a.names["/w"], "Mine");
        a.set_name("/w", "  ");
        assert!(!a.names.contains_key("/w"));
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
}
