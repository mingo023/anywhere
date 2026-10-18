pub mod automations;
pub mod locals;
pub mod providers;

use automations::{Automation, AutomationDraft, Automations, Run};
use locals::Local;
use providers::ProviderInfo;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{HashMap, VecDeque};
use std::io::ErrorKind;
#[cfg(unix)]
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tungstenite::Message;
#[cfg(windows)]
use uds_windows::UnixStream;

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
    pub activity: String,
    /// The user's latest prompt in this conversation, cut short by pocketd; empty before the first.
    pub prompt: String,
    /// When `prompt` reached pocketd, in Unix ms.
    pub prompt_at: i64,
    pub created_at: i64,
    pub updated_at: i64,
    /// The account it runs on; empty is the built-in login.
    pub account: String,
    /// Bumped each time pocketd starts the agent's timeline over, as for a new conversation.
    pub resets: u64,
    /// Run without its TUI, so only the chat drives it.
    pub chat_only: bool,
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

/// Judged on the whole percent against `warn` and `critical`, so 74.99% is still Low at 75.
pub fn level(used: u64, window: u64, warn: u32, critical: u32) -> Level {
    match percent(used, window) {
        p if p >= u64::from(critical) => Level::Danger,
        p if p >= u64::from(warn) => Level::Warn,
        _ => Level::Low,
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
    /// A result's: whether the turn succeeded.
    pub ok: bool,
    /// A tasks item's todos.
    pub items: Vec<TaskItem>,
    /// A compact's: "manual" or "auto".
    pub trigger: String,
    /// The tool-use id of the sub-agent call this item belongs to.
    pub parent_tool_use_id: Option<String>,
}

#[derive(Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Call {
    pub tool_use_id: String,
    pub name: String,
    /// "running", "ok" or "error".
    pub status: String,
    pub output: Option<String>,
    pub duration_ms: Option<u64>,
    pub detail: Detail,
}

#[derive(Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
}

#[derive(Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct TaskItem {
    pub text: String,
    /// "pending", "in_progress" or "completed".
    pub status: String,
}

/// A preview of an edit or write: pocketd keeps at most 24 lines, but counts the whole diff.
#[derive(Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct FileDiff {
    pub lines: Vec<DiffLine>,
    pub additions: u32,
    pub deletions: u32,
    /// A write with no prior contents, so every line reads as added.
    pub content_only: bool,
}

#[derive(Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct DiffLine {
    pub number: Option<u32>,
    /// "add", "del" or "context".
    pub kind: String,
    pub text: String,
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
    pub diff: Option<FileDiff>,
    /// An `other` tool's whole input.
    pub input: Value,
}

impl Detail {
    /// AskUserQuestion's, from the tool's input; an input shaped otherwise has none.
    pub fn questions(&self) -> Vec<Question> {
        self.input.get("questions").and_then(|q| Vec::deserialize(q).ok()).unwrap_or_default()
    }
}

/// One of AskUserQuestion's questions.
#[derive(Deserialize, Serialize, Default, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Question {
    pub question: String,
    pub header: String,
    pub options: Vec<Choice>,
    pub multi_select: bool,
}

#[derive(Deserialize, Serialize, Default, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Choice {
    pub label: String,
    pub description: String,
}

#[derive(Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Permission {
    pub request_id: String,
    pub agent_id: String,
    pub tool_name: String,
    pub detail: Detail,
    /// Whether a deny can carry a message back to the agent.
    pub feedback: bool,
}

impl Permission {
    pub fn ask(&self) -> String {
        let (verb, what) = self.parts();
        format!("{verb} {what}")
    }

    /// The ask split as ("Wants to run", "ls /tmp").
    pub fn parts(&self) -> (String, String) {
        let d = &self.detail;
        let (verb, what) = match d.kind.as_str() {
            "shell" => ("run".into(), &d.command),
            "edit" | "write" | "read" => (d.kind.clone(), &d.path),
            "search" => ("search".into(), &d.query),
            "task" => ("start".into(), &d.description),
            _ => ("use".into(), if d.name.is_empty() { &self.tool_name } else { &d.name }),
        };
        (format!("Wants to {verb}"), what.clone())
    }

    /// AskUserQuestion's questions; none for any other ask.
    pub fn questions(&self) -> Vec<Question> {
        if self.tool_name != "AskUserQuestion" {
            return vec![];
        }
        self.detail.questions()
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
    providers: Vec<ProviderInfo>,
    step: String,
    note: String,
    names: HashMap<String, String>,
    locals: Vec<Local>,
    version: String,
    #[serde(deserialize_with = "automations::known")]
    automations: Vec<Automation>,
    runs: Vec<Run>,
}

/// What pocketd reports about the Mac it runs on.
#[derive(Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Host {
    pub tailnet: bool,
    pub keeping_awake: bool,
    /// The version pocketd last failed to upgrade to; empty when none failed.
    pub upgrade_failed: String,
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
    /// hello.ok arrived with the scopes pocketd granted this conn, the caps both sides speak and pocketd's build.
    Connected { scopes: Vec<String>, caps: Vec<String>, version: String },
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
    Providers { phone_max: String, providers: Vec<ProviderInfo> },
    ConfigFailed(String),
    /// pocketd couldn't switch an agent's model or effort.
    SettingsFailed(String),
    /// Every worktree's display name by path, sent whole.
    Names(HashMap<String, String>),
    /// pocketd couldn't name the session from its prompt.
    NamingFailed(String),
    /// Every Local the user added, sent whole.
    Locals(Vec<Local>),
    /// Every automation and run, sent whole.
    Automations { automations: Vec<Automation>, runs: Vec<Run> },
    /// pocketd refused an automation message.
    AutomationError(String),
    /// pocketd refused the prompt sent under `request`, or the socket never took it; it isn't sent again, so it can't be typed twice.
    PromptFailed { agent: String, request: String },
    /// pocketd refused to stop the agent's turn, or the socket never took the stop; `reason` says why.
    InterruptFailed { agent: String, reason: String },
}

#[derive(Default)]
pub struct Agents {
    pub list: Vec<Summary>,
    pub timelines: HashMap<String, Vec<Item>>,
    /// Bumped each time an agent's timeline takes items, so views refold only when it moved.
    pub revisions: HashMap<String, u64>,
    pub pending: Vec<Permission>,
    /// `None` until pocketd reports it, and again after each reconnect.
    pub host: Option<Host>,
    pub scopes: Vec<String>,
    /// The most a paired phone may start a session with, from `agent.providers`.
    pub phone_max: String,
    /// What each provider takes, from `agent.providers`.
    pub providers: Vec<ProviderInfo>,
    pub caps: Vec<String>,
    /// Worktree display names by path, from `worktree.names`.
    pub names: HashMap<String, String>,
    /// The Locals the user added, in the order they were made, from `local.list`.
    pub locals: Vec<Local>,
    /// pocketd's build from `hello.ok`: the release tag, or a VCS stamp in dev.
    pub version: String,
    pub automations: Automations,
}

impl Agents {
    pub fn get(&self, id: &str) -> Option<&Summary> {
        self.list.iter().find(|a| a.id == id)
    }

    pub fn revision(&self, id: &str) -> u64 {
        self.revisions.get(id).copied().unwrap_or_default()
    }

    pub fn in_terminal(&self, terminal: &str) -> Option<&Summary> {
        self.list.iter().find(|a| a.terminal_id == terminal)
    }

    /// pocketd's word on provider `id`, else what this build knows.
    pub fn provider_info(&self, id: &str) -> &ProviderInfo {
        self.providers.iter().find(|p| p.id == id).unwrap_or_else(|| providers::builtin(id))
    }

    pub fn apply(&mut self, ev: Event) {
        self.apply_from(None, ev);
    }

    /// Drops all that host `source` sent, once it's taken off this Mac.
    pub fn forget(&mut self, source: &str) {
        self.apply_from(Some(source), Event::Agents(Vec::new()));
        self.apply_from(Some(source), Event::Connected { scopes: Vec::new(), caps: Vec::new(), version: String::new() });
        self.timelines.retain(|id, _| id.split_once('/').map(|(h, _)| h) != Some(source));
    }

    /// Applies an event, already tagged, from this Mac's pocketd (`None`) or host `source`'s:
    /// what one pocketd sends whole replaces only its own share.
    pub fn apply_from(&mut self, source: Option<&str>, ev: Event) {
        let mine = |id: &str| id.split_once('/').map(|(h, _)| h) == source;
        let my_name = |path: &str| path.strip_prefix("//").and_then(|p| p.split_once('/')).map(|(h, _)| h) == source;
        match ev {
            Event::Agents(list) => {
                list.iter().for_each(|a| self.drop_if_reset(a));
                self.list.retain(|a| !mine(&a.id));
                self.list.extend(list);
            }
            Event::Connected { .. } if source.is_some() => {
                self.pending.retain(|p| !mine(&p.request_id));
                self.names.retain(|path, _| !my_name(path));
            }
            Event::Agent(a) if a.status == "closed" => self.list.retain(|x| x.id != a.id),
            Event::Agent(a) => {
                self.drop_if_reset(&a);
                match self.list.iter_mut().find(|x| x.id == a.id) {
                    Some(x) => *x = a,
                    None => self.list.push(a),
                }
            }
            Event::Items(id, items) => {
                *self.revisions.entry(id.clone()).or_default() += 1;
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
            Event::Connected { scopes, caps, version } => {
                self.pending.retain(|p| !mine(&p.request_id));
                self.host = None;
                self.scopes = scopes;
                self.caps = caps;
                self.version = version;
                self.names.retain(|path, _| !my_name(path));
                self.locals.clear();
            }
            Event::Host(h) => self.host = Some(h),
            Event::PairCode { .. } | Event::Paired(_) | Event::PairFailed(_) => {}
            Event::Providers { phone_max, providers } => {
                self.phone_max = phone_max;
                self.providers = providers;
            }
            Event::Creating { .. } | Event::Progress { .. } | Event::Created { .. } | Event::CreateFailed { .. } | Event::ConfigFailed(_) | Event::SettingsFailed(_) | Event::NamingFailed(_) => {}
            Event::Names(names) => {
                self.names.retain(|path, _| !my_name(path));
                self.names.extend(names);
            }
            Event::Locals(locals) => self.locals = locals,
            Event::Automations { automations, runs } => self.automations.apply(automations, runs),
            Event::AutomationError(_) | Event::PromptFailed { .. } | Event::InterruptFailed { .. } => {}
        }
    }

    /// Drops the items held for `a` once pocketd started its timeline over, so the old conversation doesn't stay above the new one.
    fn drop_if_reset(&mut self, a: &Summary) {
        if self.get(&a.id).is_some_and(|x| x.resets != a.resets) && self.timelines.remove(&a.id).is_some() {
            *self.revisions.entry(a.id.clone()).or_default() += 1;
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

    /// pocketd names worktrees from their prompt and takes this client's renames.
    pub fn names_offered(&self) -> bool {
        self.caps.iter().any(|c| c == NAMES_CAP) && !self.observe_only()
    }

    /// pocketd keeps Locals and takes this client's changes to them.
    pub fn locals_offered(&self) -> bool {
        self.caps.iter().any(|c| c == LOCALS_CAP) && !self.observe_only()
    }

    /// Whether pocketd can rename the tree `key`, a Local's id or a worktree's path.
    pub fn renames_offered(&self, key: &str) -> bool {
        if locals::is_local(key) { self.locals_offered() } else { self.names_offered() }
    }

    pub fn local(&self, id: &str) -> Option<&Local> {
        self.locals.iter().find(|l| l.id == id)
    }

    /// The tree `key`'s name, if it has one: a Local's own, else a worktree's from renaming or naming.
    pub fn given_name(&self, key: &str) -> Option<&str> {
        self.local(key).map(|l| l.name.as_str()).or_else(|| self.names.get(key).map(String::as_str).filter(|n| !n.is_empty()))
    }

    pub fn locals_in<'a>(&'a self, project: &'a str) -> impl Iterator<Item = &'a Local> {
        self.locals.iter().filter(move |l| l.project == project)
    }

    /// The ids of the Locals removing `project` deletes: none when pocketd won't take this client's deletes.
    pub fn locals_removed_with<'a>(&'a self, project: &'a str) -> impl Iterator<Item = &'a str> {
        self.locals_offered().then(|| self.locals_in(project)).into_iter().flatten().map(|l| l.id.as_str())
    }

    /// Shows a new Local before pocketd lists it.
    pub fn add_local(&mut self, local: Local) {
        self.locals.push(local);
    }

    /// Shows a Local's new name before pocketd lists it; a blank one changes nothing.
    pub fn rename_local(&mut self, id: &str, title: &str) {
        if let Some(l) = self.locals.iter_mut().find(|l| l.id == id).filter(|_| !title.trim().is_empty()) {
            l.name = title.trim().to_string();
        }
    }

    pub fn remove_local(&mut self, id: &str) {
        self.locals.retain(|l| l.id != id);
    }

    /// pocketd runs automations and takes their messages.
    pub fn automations_offered(&self) -> bool {
        self.caps.iter().any(|c| c == AUTOMATIONS_CAP)
    }

    /// pocketd starts a session as a copy of a provider session.
    pub fn fork_offered(&self) -> bool {
        self.caps.iter().any(|c| c == FORK_CAP)
    }

    /// pocketd switches a running agent's model or effort.
    pub fn settings_offered(&self) -> bool {
        self.caps.iter().any(|c| c == SETTINGS_CAP) && !self.observe_only()
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
        self.timelines.get(id)?.iter().rev().find(|i| i.kind == "result" && i.parent_tool_use_id.is_none())
    }

    /// The agent whose timeline last edited or wrote `path`, and when; tool paths may be absolute or relative to its cwd.
    pub fn last_edit(&self, path: &str) -> Option<(&Summary, i64)> {
        let touches = |d: &Detail, cwd: &str| {
            (d.kind == "edit" || d.kind == "write") && !d.path.is_empty() && (d.path == path || path.strip_prefix(cwd).and_then(|p| p.strip_prefix('/')).is_some_and(|rel| rel == d.path))
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
        self.timelines.get(id)?.iter().rev().find(|i| i.kind == "assistant" && i.parent_tool_use_id.is_none()).map(|i| i.text.as_str())
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

#[derive(Deserialize)]
struct CodexModel {
    slug: String,
    #[serde(default)]
    display_name: String,
    #[serde(default)]
    visibility: String,
    #[serde(default)]
    supported_reasoning_levels: Vec<CodexLevel>,
}

#[derive(Deserialize)]
struct CodexLevel {
    effort: String,
}

/// The models `codex debug models` lists; hidden ones are left out.
fn codex_catalog(json: &str) -> Vec<CodexModel> {
    #[derive(Deserialize)]
    struct Catalog {
        models: Vec<CodexModel>,
    }
    serde_json::from_str::<Catalog>(json).map_or_else(|_| Vec::new(), |c| c.models.into_iter().filter(|m| m.visibility == "list").collect())
}

/// The models `codex debug models` lists, as (slug, name).
pub fn codex_models(json: &str) -> Vec<(String, String)> {
    codex_catalog(json).into_iter().map(|m| (m.slug.clone(), if m.display_name.is_empty() { m.slug } else { m.display_name })).collect()
}

/// Each listed model's efforts, as (slug, efforts).
pub fn codex_efforts(json: &str) -> Vec<(String, Vec<String>)> {
    codex_catalog(json).into_iter().map(|m| (m.slug, m.supported_reasoning_levels.into_iter().map(|l| l.effort).collect())).collect()
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
const SETTINGS: &str = "settings";
const AUTO: &str = "auto-";
const PROMPT: &str = "prompt-";
const INTERRUPT: &str = "interrupt-";
/// Why a prompt or stop the socket never took failed.
const UNSENT: &str = "Lost the connection to pocketd";

static NEXT: AtomicU64 = AtomicU64::new(0);

fn auto_id() -> String {
    format!("{AUTO}{}", NEXT.fetch_add(1, Ordering::Relaxed))
}

fn launch_event(f: &Frame) -> Option<Event> {
    Some(match f.kind.as_str() {
        "agent.creating" => Event::Creating { request: f.request_id.clone(), terminal: f.terminal_id.clone(), cwd: f.cwd.clone(), setup: f.setup },
        "agent.progress" => Event::Progress { request: f.request_id.clone(), step: f.step.clone(), note: f.note.clone() },
        "agent.created" => Event::Created { request: f.request_id.clone(), agent: f.agent_id.clone() },
        "agent.providers" => Event::Providers { phone_max: f.phone_max_access.clone(), providers: f.providers.clone() },
        "error" if f.id == CONFIG => Event::ConfigFailed(f.message.clone()),
        "error" if f.id == SETTINGS => Event::SettingsFailed(f.message.clone()),
        "error" => Event::CreateFailed { request: f.id.strip_prefix(CREATE)?.to_string(), code: f.code.clone(), message: readable(&f.message), detail: f.detail.clone() },
        _ => return None,
    })
}

/// A pocketd older than this app rejects a create it can't parse as malformed.
pub const OUTDATED: &str = "Anywhere's background service is out of date. Restart it to update.";

fn readable(message: &str) -> String {
    if message == "Malformed message" { OUTDATED.into() } else { message.into() }
}

fn names_event(f: &Frame) -> Option<Event> {
    match f.kind.as_str() {
        "worktree.names" => Some(Event::Names(f.names.clone())),
        "naming.failed" => Some(Event::NamingFailed(f.agent_id.clone())),
        "local.list" => Some(Event::Locals(f.locals.clone())),
        _ => None,
    }
}
const PAIR_CAP: &str = "pair.v1";
const OPEN_CAP: &str = "open.v1";
const NAMES_CAP: &str = "names.v1";
const AUTOMATIONS_CAP: &str = "automations.v1";
const LOCALS_CAP: &str = "locals.v1";
const FORK_CAP: &str = "fork.v1";
const ACCOUNTS_CAP: &str = "accounts.v1";
const SETTINGS_CAP: &str = "settings.v1";

fn automation_event(f: &Frame) -> Option<Event> {
    match f.kind.as_str() {
        "automations" => Some(Event::Automations { automations: f.automations.clone(), runs: f.runs.clone() }),
        "error" if f.id.starts_with(AUTO) => Some(Event::AutomationError(readable(&f.message))),
        _ => None,
    }
}

/**
 * A new create's requestId. It starts with the clock because pocketd replays the receipt of a
 * requestId it has seen, even from before a restart. Each reconnect resends the create under the
 * same requestId until its final reply, which joins the first create.
 */
pub fn request_id() -> String {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    format!("{nanos:x}-{}", NEXT.fetch_add(1, Ordering::Relaxed))
}

/// Client messages for pocketd. They wait in a queue while it is unreachable.
#[derive(Clone)]
pub struct Outbox(Sender<Value>);

impl From<Sender<Value>> for Outbox {
    fn from(tx: Sender<Value>) -> Self {
        Self(tx)
    }
}

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

    /// Types `text` into the agent's terminal and presses Enter; returns the request id a refusal names.
    pub fn prompt(&self, agent_id: &str, text: &str) -> String {
        let id = format!("{PROMPT}{}", NEXT.fetch_add(1, Ordering::Relaxed));
        self.send(json!({"type": "agent.prompt", "id": id, "agentId": agent_id, "text": text}));
        id
    }

    pub fn interrupt(&self, agent_id: &str) {
        let id = format!("{INTERRUPT}{}", NEXT.fetch_add(1, Ordering::Relaxed));
        self.send(json!({"type": "agent.interrupt", "id": id, "agentId": agent_id}));
    }

    pub fn compact(&self, agent_id: &str) {
        self.send(json!({"type": "agent.compact", "id": "compact", "agentId": agent_id}));
    }

    /// Switches the agent's model, effort or both; "" keeps one as it is.
    pub fn settings(&self, agent_id: &str, model: &str, effort: &str) {
        let mut msg = json!({"type": "agent.settings", "id": SETTINGS, "agentId": agent_id});
        for (key, value) in [("model", model), ("effort", effort)].into_iter().filter(|(_, v)| !v.is_empty()) {
            msg[key] = value.into();
        }
        self.send(msg);
    }

    pub fn resolve(&self, request_id: &str, decision: Decision) {
        let decision = match decision {
            Decision::Allow => "allow",
            Decision::Deny => "deny",
        };
        self.send(json!({"type": "permission.resolve", "id": "resolve", "requestId": request_id, "decision": decision}));
    }

    /// Allows an AskUserQuestion with `answers`, keyed by question, in place of its dialog's.
    pub fn answer(&self, request_id: &str, answers: &HashMap<String, String>) {
        self.send(json!({"type": "permission.resolve", "id": "resolve", "requestId": request_id, "decision": "allow", "answers": answers}));
    }

    /// Denies the ask with `message`, which pocketd types as the agent's next prompt.
    pub fn reply(&self, request_id: &str, message: &str) {
        self.send(json!({"type": "permission.resolve", "id": "resolve", "requestId": request_id, "decision": "deny", "message": message}));
    }

    pub fn pair_begin(&self) {
        self.send(json!({"type": "pair.begin", "id": PAIR}));
    }

    /// Sends `agent.create` and returns its requestId.
    pub fn create(&self, spec: Value) -> String {
        let request = request_id();
        self.create_as(&request, spec);
        request
    }

    /// Sends `agent.create` under `request`, one from `request_id`.
    pub fn create_as(&self, request: &str, spec: Value) {
        self.send(json!({"type": "agent.create", "id": format!("{CREATE}{request}"), "requestId": request, "spec": spec}));
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

    pub fn local_create(&self, local: &Local) {
        self.send(json!({"type": "local.create", "id": "local", "localId": local.id, "project": local.project, "name": local.name}));
    }

    pub fn local_rename(&self, id: &str, title: &str) {
        self.send(json!({"type": "local.rename", "id": "local", "localId": id, "title": title}));
    }

    /// Deletes the Local and closes its terminals; its files stay.
    pub fn local_delete(&self, id: &str) {
        self.send(json!({"type": "local.delete", "id": "local", "localId": id}));
    }

    /// Creates the automation, or replaces the one with the draft's `id`.
    pub fn automation_save(&self, draft: AutomationDraft) {
        self.send(json!({"type": "automation.save", "id": auto_id(), "automation": draft}));
    }

    pub fn automation_enable(&self, automation_id: &str, enabled: bool) {
        self.send(json!({"type": "automation.enable", "id": auto_id(), "automationId": automation_id, "enabled": enabled}));
    }

    pub fn automation_delete(&self, automation_id: &str) {
        self.send(json!({"type": "automation.delete", "id": auto_id(), "automationId": automation_id}));
    }

    pub fn automation_run(&self, automation_id: &str) {
        self.send(json!({"type": "automation.run", "id": auto_id(), "automationId": automation_id}));
    }

    fn send(&self, m: Value) {
        let _ = self.0.send(m);
    }
}

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
        loop {
            let _ = run(&sock, token.as_deref(), &tx, &queue, &mut unanswered);
            if tx.is_closed() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_secs(2));
        }
    });
    (Outbox(out), rx)
}

/// What this client understands beyond protocol 3.
const CAPS: [&str; 12] = [PAIR_CAP, "scopes.v1", "summary.v2", "host.v1", OPEN_CAP, NAMES_CAP, AUTOMATIONS_CAP, LOCALS_CAP, FORK_CAP, ACCOUNTS_CAP, "subagents.v1", SETTINGS_CAP];

fn hello(token: Option<&str>) -> Value {
    let mut hello = json!({"type": "hello", "id": "h", "clientId": "desktop", "protocolVersion": 3, "caps": CAPS});
    if let Some(token) = token {
        hello["ownerToken"] = token.into();
    }
    hello
}

fn run(sock: &Path, token: Option<&str>, tx: &UnboundedSender<Event>, queue: &Receiver<Value>, unanswered: &mut Vec<Value>) -> Option<()> {
    let stream = UnixStream::connect(sock).ok()?;
    let (mut ws, _) = tungstenite::client("ws://localhost/", stream).ok()?;
    // Reads time out so the loop gets to send the queue even while pocketd is quiet.
    ws.get_ref().set_read_timeout(Some(Duration::from_millis(100))).ok()?;
    ws.send(Message::text(hello(token).to_string())).ok()?;
    for m in unanswered.iter() {
        ws.send(Message::text(m.to_string())).ok()?;
    }
    let mut known: Vec<String> = Vec::new();
    let mut requests = Requests::default();
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
        if let Some(ev) = requests.answer(&f).or_else(|| launch_event(&f)).or_else(|| names_event(&f)).or_else(|| automation_event(&f)) {
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

/// Prompts and stops written to pocketd and not yet answered, so a refusal names the one it refused.
#[derive(Default)]
struct Requests(VecDeque<Value>);

impl Requests {
    fn sent(&mut self, m: &Value) {
        if matches!(m["type"].as_str(), Some("agent.prompt" | "agent.interrupt")) {
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
}

/// The event for a prompt or stop `m` that failed for `reason`; none for other messages.
fn refused(m: &Value, reason: &str) -> Option<Event> {
    let field = |key: &str| m[key].as_str().unwrap_or_default().to_string();
    match m["type"].as_str()? {
        "agent.prompt" => Some(Event::PromptFailed { agent: field("agentId"), request: field("id") }),
        "agent.interrupt" => Some(Event::InterruptFailed { agent: field("agentId"), reason: reason.into() }),
        _ => None,
    }
}

/// A create sent to pocketd, one at a time, and the failure it answered with until the next send.
#[derive(Default)]
pub struct CreateReply {
    pending: Option<String>,
    pub error: Option<(String, String)>,
}

impl CreateReply {
    pub fn waiting(&self) -> bool {
        self.pending.is_some()
    }

    pub fn sent(&mut self, request: String) {
        (self.pending, self.error) = (Some(request), None);
    }

    /// Whether `request` was the one sent.
    pub fn answer(&mut self, request: &str, error: Option<(String, String)>) -> bool {
        if self.pending.as_deref() != Some(request) {
            return false;
        }
        (self.pending, self.error) = (None, error);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use std::os::unix::net::UnixListener;
    #[cfg(windows)]
    use uds_windows::UnixListener;
    use std::path::PathBuf;

    fn summary(provider: &str, model: Option<&str>) -> Summary {
        Summary { provider: provider.into(), model: model.map(Into::into), ..Default::default() }
    }

    #[test]
    fn the_hello_carries_an_owner_token_only_when_there_is_one() {
        assert!(hello(None).get("ownerToken").is_none());
        assert_eq!(hello(Some("t1"))["ownerToken"], "t1");
    }

    #[test]
    fn codex_lists_its_shown_models_by_name_and_nothing_from_bad_output() {
        let json = r#"{"models":[{"slug":"gpt-6-sol","display_name":"GPT-6-Sol","visibility":"list"},{"slug":"gpt-reserve","display_name":"GPT-Reserve","visibility":"hide"},{"slug":"gpt-5.6-luna","visibility":"list"}]}"#;
        assert_eq!(super::codex_models(json), [("gpt-6-sol".to_string(), "GPT-6-Sol".to_string()), ("gpt-5.6-luna".to_string(), "gpt-5.6-luna".to_string())]);
        assert!(super::codex_models("error: not logged in").is_empty());
    }

    #[test]
    fn codex_lists_each_shown_models_efforts() {
        let json = r#"{"models":[{"slug":"gpt-6-sol","visibility":"list","supported_reasoning_levels":[{"effort":"low","description":"Fast"},{"effort":"ultra","description":"Most"}]},{"slug":"gpt-reserve","visibility":"hide"},{"slug":"gpt-5.6-luna","visibility":"list"}]}"#;
        assert_eq!(super::codex_efforts(json), [("gpt-6-sol".to_string(), vec!["low".to_string(), "ultra".to_string()]), ("gpt-5.6-luna".to_string(), vec![])]);
    }

    #[test]
    fn labels_models_like_the_picker() {
        assert_eq!(model_label(&summary("claude", Some("claude-opus-5"))), "Opus 5");
        assert_eq!(model_label(&summary("claude", Some("claude-opus-5-5"))), "Opus 5.5");
        assert_eq!(model_label(&summary("claude", Some("claude-haiku-4-5-20251001"))), "Haiku 4.5");
        assert_eq!(model_label(&summary("codex", None)), "Codex");
    }

    #[test]
    fn an_agent_summary_without_activity_reads_it_as_empty() {
        let s: Summary = serde_json::from_value(json!({ "id": "a", "status": "working" })).unwrap();
        assert_eq!(s.activity, "");
    }

    #[test]
    fn an_agent_summary_carries_its_activity() {
        let s: Summary = serde_json::from_value(json!({ "id": "a", "activity": "Running tests" })).unwrap();
        assert_eq!(s.activity, "Running tests");
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
    fn a_new_conversation_drops_the_old_items() {
        let mut a = Agents::default();
        let agent = |id: &str, resets| Summary { id: id.into(), resets, ..Default::default() };
        let item = |id: &str, seq, text: &str| Item { id: id.into(), seq, kind: "assistant".into(), text: text.into(), ..Default::default() };
        a.apply(Event::Agents(vec![agent("a", 0), agent("b", 0)]));
        a.apply(Event::Items("a".into(), vec![item("1", 1, "old")]));
        a.apply(Event::Items("b".into(), vec![item("1", 1, "kept")]));
        a.apply(Event::Agent(agent("a", 0)));
        assert_eq!(a.timelines["a"].len(), 1, "the same conversation keeps its items");
        let before = a.revision("a");
        a.apply(Event::Agent(agent("a", 1)));
        assert!(!a.timelines.contains_key("a") && a.revision("a") > before, "the old conversation is gone and views refold");
        a.apply(Event::Items("a".into(), vec![item("2", 2, "new")]));
        a.apply(Event::Agents(vec![agent("a", 1), agent("b", 2)]));
        let texts = |id: &str| a.timelines.get(id).map(|t| t.iter().map(|i| i.text.clone()).collect::<Vec<_>>()).unwrap_or_default();
        assert_eq!(texts("a"), ["new"]);
        assert!(texts("b").is_empty(), "a reset seen in a whole list drops too");
    }

    #[test]
    fn a_sub_agents_text_is_not_its_parents_last_word() {
        let mut a = Agents::default();
        let text = |id: &str, seq, parent: Option<&str>| Item { id: id.into(), seq, kind: "assistant".into(), text: id.into(), parent_tool_use_id: parent.map(Into::into), ..Default::default() };
        a.apply(Event::Items("a".into(), vec![text("mine", 1, None), text("theirs", 2, Some("tool"))]));
        assert_eq!(a.last_text("a"), Some("mine"));
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
        a.apply(Event::Connected { scopes: vec![], caps: vec![], version: String::new() });
        assert!(a.pending.is_empty());
    }

    #[test]
    fn decodes_a_v3_summary() {
        let f: Frame = serde_json::from_str(include_str!("../../../../pocketd/internal/proto/testdata/golden/server/agent_update_failed.json")).unwrap();
        let a = f.agent.unwrap();
        assert_eq!((a.terminal_id.as_str(), a.status.as_str(), a.failed, a.attached, a.compacting), ("t1", "done", true, true, false));
    }

    #[test]
    fn sub_agent_items_decode_with_the_call_that_spawned_them() {
        let f: Frame = serde_json::from_str(include_str!("../../../../pocketd/internal/proto/testdata/golden/server/stream_subagent.json")).unwrap();
        assert_eq!(f.item.unwrap().parent_tool_use_id.as_deref(), Some("t4"));
        let f: Frame = serde_json::from_str(include_str!("../../../../pocketd/internal/proto/testdata/golden/server/timeline_subagents.json")).unwrap();
        let parents: Vec<_> = f.items.iter().map(|i| i.parent_tool_use_id.as_deref()).collect();
        assert_eq!(parents, [None, None, None, Some("toolu_list"), Some("toolu_list"), Some("toolu_list"), Some("toolu_count"), Some("toolu_count"), None, None]);
    }

    #[test]
    fn summary_reads_the_provider_session_id() {
        let a: Summary = serde_json::from_str(r#"{"id":"a","providerSessionId":"s-1"}"#).unwrap();
        assert_eq!(a.provider_session_id.as_deref(), Some("s-1"));
        let fresh: Summary = serde_json::from_str(r#"{"id":"a"}"#).unwrap();
        assert_eq!(fresh.provider_session_id, None);
    }

    #[test]
    fn summary_reads_the_latest_prompt_and_an_older_pocketd_sends_none() {
        let a: Summary = serde_json::from_str(r#"{"id":"a","prompt":"fix the login","promptAt":1760670000000}"#).unwrap();
        assert_eq!((a.prompt.as_str(), a.prompt_at), ("fix the login", 1760670000000));
        let older: Summary = serde_json::from_str(r#"{"id":"a"}"#).unwrap();
        assert_eq!(older.prompt, "");
    }

    #[test]
    fn level_is_low_at_74_warn_at_75_and_89_danger_at_90() {
        let at = |used| level(used, 200_000, 75, 90);
        assert_eq!([at(148_000), at(149_998), at(150_000), at(179_998), at(180_000), at(400_000)], [Level::Low, Level::Low, Level::Warn, Level::Warn, Level::Danger, Level::Danger]);
    }

    #[test]
    fn level_follows_the_thresholds_it_is_given() {
        let at = |used| level(used, 100, 50, 60);
        assert_eq!([at(49), at(50), at(59), at(60)], [Level::Low, Level::Warn, Level::Warn, Level::Danger]);
    }

    #[test]
    fn an_empty_window_is_zero_percent_full() {
        assert_eq!((percent(184_000, 0), level(184_000, 0, 75, 90)), (0, Level::Low));
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
            json!({"type": "hello", "id": "h", "clientId": "desktop", "protocolVersion": 3, "caps": ["pair.v1", "scopes.v1", "summary.v2", "host.v1", "open.v1", "names.v1", "automations.v1", "locals.v1", "fork.v1", "accounts.v1", "subagents.v1", "settings.v1"]})
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
    fn an_observer_is_offered_no_renames_or_locals_pocketd_would_refuse() {
        let mut a = Agents::default();
        let caps = vec![NAMES_CAP.to_string(), LOCALS_CAP.to_string()];
        a.apply(Event::Connected { scopes: vec!["owner".into()], caps: caps.clone(), version: String::new() });
        assert!(a.names_offered() && a.locals_offered());
        a.apply(Event::Connected { scopes: vec!["observe".into()], caps, version: String::new() });
        assert!(!a.names_offered() && !a.locals_offered());
    }

    #[test]
    fn a_tree_is_given_its_locals_name_else_its_rename() {
        let mut a = Agents::default();
        a.add_local(Local { id: "l1".into(), project: "/p".into(), name: "Review".into() });
        a.set_name("/p", "Hotfix");
        assert_eq!((a.given_name("l1"), a.given_name("/p"), a.given_name("/p-login")), (Some("Review"), Some("Hotfix"), None));
    }

    #[test]
    fn removing_a_project_deletes_its_locals_only_when_pocketd_takes_the_deletes() {
        let mut a = Agents::default();
        let locals = vec![
            Local { id: "l1".into(), project: "/p".into(), name: "Local 2".into() },
            Local { id: "l2".into(), project: "/q".into(), name: "Local 2".into() },
        ];
        let caps = vec![LOCALS_CAP.to_string()];
        a.apply(Event::Connected { scopes: vec!["owner".into()], caps: caps.clone(), version: String::new() });
        a.apply(Event::Locals(locals.clone()));
        assert_eq!(a.locals_removed_with("/p").collect::<Vec<_>>(), ["l1"]);
        a.apply(Event::Connected { scopes: vec!["observe".into()], caps, version: String::new() });
        a.apply(Event::Locals(locals));
        assert_eq!(a.locals_removed_with("/p").count(), 0);
    }

    #[test]
    fn opening_branches_waits_for_pocketd_to_offer_it() {
        let mut a = Agents::default();
        assert!(!a.opens());
        a.apply(Event::Connected { scopes: vec![], caps: vec!["open.v1".into()], version: String::new() });
        assert!(a.opens());
    }

    #[test]
    fn hello_ok_host_and_host_changed_become_host_events() {
        let mut a = Agents::default();
        let frame = |raw: &str| serde_json::from_str::<Frame>(raw).unwrap();
        a.apply(host_event(&frame(r#"{"type":"hello.ok","host":{"tailnet":false,"keepingAwake":true}}"#)).unwrap());
        assert_eq!(a.host, Some(Host { tailnet: false, keeping_awake: true, ..Default::default() }));
        a.apply(host_event(&frame(r#"{"type":"host.changed","host":{"tailnet":true,"keepingAwake":false}}"#)).unwrap());
        assert_eq!(a.host, Some(Host { tailnet: true, keeping_awake: false, ..Default::default() }));
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
        assert_eq!(a.host, Some(Host { tailnet: true, keeping_awake: false, ..Default::default() }));
        ws.send(Message::text(changed)).unwrap();
        a.apply(got.next().unwrap());
        assert_eq!(a.host, Some(Host { tailnet: true, keeping_awake: true, ..Default::default() }));
        std::fs::remove_file(&sock).unwrap();
    }

    #[test]
    fn connecting_clears_the_host() {
        let mut a = Agents { host: Some(Host::default()), ..Default::default() };
        a.apply(Event::Connected { scopes: vec![], caps: vec![], version: String::new() });
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
    fn reply_denies_with_the_message() {
        let (tx, rx) = channel();
        Outbox(tx).reply("q1", "Use pnpm instead");
        let sent: Vec<Value> = rx.try_iter().collect();
        assert_eq!(sent, vec![json!({"type": "permission.resolve", "id": "resolve", "requestId": "q1", "decision": "deny", "message": "Use pnpm instead"})]);
    }

    #[test]
    fn answer_allows_with_the_answers() {
        let (tx, rx) = channel();
        Outbox(tx).answer("q1", &HashMap::from([("Which color?".into(), "Blue".into())]));
        let sent: Vec<Value> = rx.try_iter().collect();
        assert_eq!(
            sent,
            vec![json!({"type": "permission.resolve", "id": "resolve", "requestId": "q1", "decision": "allow", "answers": {"Which color?": "Blue"}})]
        );
    }

    #[test]
    fn an_ask_user_question_request_carries_its_questions() {
        let f: Frame = serde_json::from_value(json!({"type": "permission.request", "request": {
            "requestId": "r1", "agentId": "a1", "toolName": "AskUserQuestion",
            "detail": {"kind": "other", "name": "AskUserQuestion", "input": {"questions": [
                {"question": "Which color?", "header": "Color", "options": [{"label": "Red", "description": "Warm"}, {"label": "Blue"}], "multiSelect": true}
            ]}}
        }}))
        .unwrap();
        let q = f.request.unwrap().questions();
        assert_eq!(q.len(), 1);
        assert_eq!((q[0].question.as_str(), q[0].header.as_str(), q[0].multi_select), ("Which color?", "Color", true));
        assert_eq!(q[0].options[0], Choice { label: "Red".into(), description: "Warm".into() });
        assert_eq!(q[0].options[1].description, "");
    }

    #[test]
    fn only_ask_user_question_has_questions() {
        let p = Permission { tool_name: "Bash".into(), detail: Detail { input: json!({"questions": [{}]}), ..Default::default() }, ..Default::default() };
        assert!(p.questions().is_empty());
    }

    #[test]
    fn a_tool_input_shaped_unlike_questions_still_reads() {
        let d: Detail = serde_json::from_value(json!({"kind": "other", "name": "mcp", "input": {"questions": "free text"}})).unwrap();
        assert_eq!((d.name.as_str(), d.questions().len()), ("mcp", 0));
        let d: Detail = serde_json::from_value(json!({"kind": "other", "input": null})).unwrap();
        assert!(d.questions().is_empty());
    }

    #[test]
    fn a_permission_request_says_whether_it_takes_feedback() {
        let f: Frame = serde_json::from_str(include_str!("../../../../pocketd/internal/proto/testdata/golden/server/permission_request.json")).unwrap();
        let p = f.request.unwrap();
        assert_eq!((p.request_id.as_str(), p.feedback), ("r1", true));
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
        let Some(Event::Providers { phone_max, providers }) = ev(providers) else { panic!("not providers") };
        assert_eq!(phone_max, "ask");
        assert_eq!(providers[0].chat, Some(providers::ChatCaps { interrupt: true, compact: true, send_while_working: true, sub_agents: true }));
        assert_eq!(providers[1].chat, Some(providers::ChatCaps { interrupt: true, compact: true, send_while_working: true, sub_agents: false }));
        assert!(providers[1].efforts.is_empty());
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

    fn frame(raw: Value) -> Frame {
        serde_json::from_value(raw).unwrap()
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
    fn a_refused_config_set_becomes_config_failed() {
        let f: Frame = serde_json::from_str(r#"{"type":"error","id":"config","code":"invalid_config","message":"phone.maxAccess can't be full"}"#).unwrap();
        assert!(matches!(launch_event(&f), Some(Event::ConfigFailed(m)) if m == "phone.maxAccess can't be full"));
    }

    #[test]
    fn a_create_an_older_pocketd_cant_parse_reads_as_out_of_date() {
        let f: Frame = serde_json::from_str(r#"{"type":"error","id":"create-1","message":"Malformed message"}"#).unwrap();
        assert!(matches!(launch_event(&f), Some(Event::CreateFailed { message, .. }) if message == OUTDATED));
        let f: Frame = serde_json::from_str(r#"{"type":"hello.ok","version":"0.1.0"}"#).unwrap();
        assert_eq!(f.version, "0.1.0");
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
        a.apply(Event::Connected { scopes: vec![], caps: vec!["names.v1".into()], version: String::new() });
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

    #[test]
    fn locals_start_over_on_each_connect_and_show_a_rename_at_once() {
        let mut a = Agents::default();
        let frame = serde_json::from_str::<Frame>(r#"{"type":"local.list","locals":[{"id":"l1","project":"/p","name":"Local 2"},{"id":"l2","project":"/q","name":"Local 2"}]}"#).unwrap();
        a.apply(names_event(&frame).unwrap());
        assert_eq!(a.locals_in("/p").map(|l| l.id.as_str()).collect::<Vec<_>>(), ["l1"]);
        a.rename_local("l1", " Fix ");
        a.rename_local("l2", "  ");
        assert_eq!((a.local("l1").unwrap().name.as_str(), a.local("l2").unwrap().name.as_str()), ("Fix", "Local 2"));
        a.apply(Event::Connected { scopes: vec![], caps: vec!["locals.v1".into()], version: String::new() });
        assert!(a.locals.is_empty() && a.locals_offered());
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

    #[test]
    fn a_host_frame_carries_a_failed_upgrade() {
        let frame = serde_json::from_str::<Frame>(r#"{"type":"host.changed","host":{"tailnet":true,"keepingAwake":false,"upgradeFailed":"1.2.0"}}"#).unwrap();
        let Some(Event::Host(host)) = host_event(&frame) else { panic!("no host event") };
        assert_eq!(host.upgrade_failed, "1.2.0");
    }

    #[test]
    fn automation_messages_carry_a_fresh_auto_request_id() {
        let (tx, rx) = channel();
        let out = Outbox(tx);
        let draft = automations::AutomationDraft {
            id: None,
            name: "Morning review".into(),
            prompt: "Review new PRs".into(),
            provider: "claude".into(),
            folder: "/app".into(),
            schedule: automations::Schedule::Interval { every_min: 60 },
            enabled: true,
            access: String::new(),
            new_worktree: false,
        };
        out.automation_save(draft);
        out.automation_enable("au1", false);
        out.automation_delete("au1");
        out.automation_run("au1");
        let sent: Vec<Value> = rx.try_iter().collect();
        assert_eq!(sent[0]["automation"], json!({"name": "Morning review", "prompt": "Review new PRs", "provider": "claude", "folder": "/app", "schedule": {"kind": "interval", "everyMin": 60}, "enabled": true}));
        let ids: Vec<&str> = sent.iter().map(|m| m["id"].as_str().unwrap()).collect();
        assert!(ids.iter().all(|id| id.starts_with("auto-")) && ids.iter().collect::<std::collections::HashSet<_>>().len() == 4);
        assert_eq!((&sent[1]["type"], &sent[1]["automationId"], &sent[1]["enabled"]), (&json!("automation.enable"), &json!("au1"), &json!(false)));
        assert_eq!((&sent[2]["type"], &sent[2]["automationId"]), (&json!("automation.delete"), &json!("au1")));
        assert_eq!((&sent[3]["type"], &sent[3]["automationId"]), (&json!("automation.run"), &json!("au1")));
    }

    #[test]
    fn automation_frames_become_automation_events() {
        let ev = |raw: &str| automation_event(&serde_json::from_str::<Frame>(raw).unwrap());
        let snapshot = include_str!("../../../../pocketd/internal/proto/testdata/golden/server/automations.json");
        assert!(matches!(ev(snapshot), Some(Event::Automations { automations, runs }) if automations.len() == 1 && runs.len() == 2));
        let busy = r#"{"type":"error","id":"auto-3","code":"automation_busy","message":"Still working"}"#;
        assert!(matches!(ev(busy), Some(Event::AutomationError(message)) if message == "Still working"));
        let old = r#"{"type":"error","id":"auto-4","message":"Malformed message"}"#;
        assert!(matches!(ev(old), Some(Event::AutomationError(message)) if message == OUTDATED));
        assert!(ev(r#"{"type":"error","id":"pair","message":"x"}"#).is_none());
    }

    /// Frames recorded from a real `pocketd serve` driving the fake claude (e2e harness).
    #[test]
    fn frames_recorded_from_a_real_pocketd_decode_to_the_run_lifecycle() {
        use automations::RunStatus::*;
        let frame = |raw: &str| serde_json::from_str::<Frame>(raw).unwrap();
        let runs = |raw: &str| match automation_event(&frame(raw)) {
            Some(Event::Automations { automations, runs }) => (automations, runs),
            _ => panic!("not a snapshot"),
        };
        let (saved, none) = runs(include_str!("../testdata/real/saved.json"));
        assert_eq!((saved.len(), none.len(), saved[0].next_run_at > 0), (1, 0, true));
        let (_, running) = runs(include_str!("../testdata/real/running.json"));
        assert_eq!((running[0].status, running[0].trigger.as_str(), running[0].agent_id.is_empty(), running[0].finished_at), (Running, "manual", false, 0));
        let (_, done) = runs(include_str!("../testdata/real/succeeded.json"));
        assert_eq!((done[0].status, done[0].summary.as_str(), done[0].finished_at > done[0].started_at), (Succeeded, "echo: review the diff", true));
        let (two, both) = runs(include_str!("../testdata/real/waiting.json"));
        let mut a = automations::Automations::default();
        a.apply(two, both);
        assert_eq!(a.waiting().count(), 1);
        let busy = automation_event(&frame(include_str!("../testdata/real/error_busy.json")));
        assert!(matches!(busy, Some(Event::AutomationError(message)) if message == "This automation is already working"));
        let invalid = automation_event(&frame(include_str!("../testdata/real/error_invalid.json")));
        assert!(matches!(invalid, Some(Event::AutomationError(message)) if message == OUTDATED));
    }

    #[test]
    fn automations_wait_for_pocketd_to_offer_them() {
        let mut a = Agents::default();
        assert!(!a.automations_offered());
        a.apply(Event::Connected { scopes: vec![], caps: vec!["automations.v1".into()], version: String::new() });
        assert!(a.automations_offered());
    }

    #[test]
    fn each_pocketds_agents_asks_and_names_survive_the_others_snapshots_and_reconnects() {
        let agent = |id: &str| Summary { id: id.into(), ..Default::default() };
        let ask = |id: &str| Permission { request_id: id.into(), ..Default::default() };
        let mut a = Agents::default();
        a.apply(Event::Agents(vec![agent("a1")]));
        a.apply(Event::Asked(ask("q1")));
        a.apply(Event::Names(HashMap::from([("/src/app".into(), "App".into())])));
        a.apply_from(Some("devbox"), Event::Agents(vec![agent("devbox/b1")]));
        a.apply_from(Some("devbox"), Event::Asked(ask("devbox/q2")));
        a.apply_from(Some("devbox"), Event::Names(HashMap::from([("//devbox/home/me/app".into(), "Box".into())])));

        a.apply(Event::Agents(vec![agent("a2")]));
        a.apply(Event::Connected { scopes: vec!["owner".into()], caps: vec![], version: "v1".into() });
        assert_eq!(a.list.iter().map(|x| x.id.as_str()).collect::<Vec<_>>(), ["devbox/b1", "a2"]);
        assert_eq!(a.pending.iter().map(|p| p.request_id.as_str()).collect::<Vec<_>>(), ["devbox/q2"]);
        assert_eq!(a.names.keys().collect::<Vec<_>>(), ["//devbox/home/me/app"]);

        a.apply(Event::Asked(ask("q3")));
        a.apply_from(Some("devbox"), Event::Connected { scopes: vec![], caps: vec![], version: "v0".into() });
        a.apply_from(Some("devbox"), Event::Agents(vec![]));
        assert_eq!(a.list.iter().map(|x| x.id.as_str()).collect::<Vec<_>>(), ["a2"]);
        assert_eq!(a.pending.iter().map(|p| p.request_id.as_str()).collect::<Vec<_>>(), ["q3"]);
        assert!(a.names.is_empty());
        assert_eq!((a.owner(), a.version.as_str()), (true, "v1"), "a host's hello says nothing about this Mac's");

        a.apply_from(Some("devbox"), Event::Agents(vec![agent("devbox/b2")]));
        a.forget("devbox");
        assert_eq!(a.list.iter().map(|x| x.id.as_str()).collect::<Vec<_>>(), ["a2"]);
    }

    #[test]
    fn an_edit_is_found_by_absolute_path_or_by_one_relative_to_the_agents_cwd() {
        let mut a = Agents::default();
        a.list.push(Summary { id: "a".into(), cwd: "/r".into(), ..Default::default() });
        let edit = |path: &str| Item { ts: 1, call: Some(Call { detail: Detail { kind: "edit".into(), path: path.into(), ..Default::default() }, ..Default::default() }), ..Default::default() };
        a.timelines.insert("a".into(), vec![edit("src/a.rs"), edit("/elsewhere/b.rs")]);
        assert!(a.last_edit("/r/src/a.rs").is_some());
        assert!(a.last_edit("/elsewhere/b.rs").is_some());
        assert!(a.last_edit("/r/src/b.rs").is_none());
        assert!(a.last_edit("/rr/src/a.rs").is_none());
    }

    macro_rules! golden {
        ($name:literal) => {
            serde_json::from_str::<Value>(include_str!(concat!("../../../../pocketd/internal/proto/testdata/golden/", $name))).unwrap()
        };
    }

    fn sent_but_id(send: impl FnOnce(&Outbox)) -> Value {
        let (tx, rx) = channel();
        send(&Outbox(tx));
        let mut sent = rx.try_iter().next().unwrap();
        sent.as_object_mut().unwrap().remove("id");
        sent
    }

    fn without_id(mut v: Value) -> Value {
        v.as_object_mut().unwrap().remove("id");
        v
    }

    fn streamed(golden: Value) -> Item {
        serde_json::from_value::<Frame>(golden).unwrap().item.unwrap()
    }

    #[test]
    fn prompt_sends_the_golden_agent_prompt_frame_but_its_id() {
        assert_eq!(sent_but_id(|o| {
            o.prompt("a1", "hi");
        }), without_id(golden!("client/agent_prompt.json")));
    }

    #[test]
    fn interrupt_sends_the_golden_agent_interrupt_frame_but_its_id() {
        assert_eq!(sent_but_id(|o| o.interrupt("a1")), without_id(golden!("client/agent_interrupt.json")));
    }

    #[test]
    fn compact_sends_the_golden_agent_compact_frame_but_its_id() {
        assert_eq!(sent_but_id(|o| o.compact("a1")), without_id(golden!("client/agent_compact.json")));
    }

    #[test]
    fn settings_sends_the_golden_agent_settings_frame_but_its_id_and_leaves_out_what_stays() {
        assert_eq!(sent_but_id(|o| o.settings("a1", "opus", "high")), without_id(golden!("client/agent_settings.json")));
        assert_eq!(sent_but_id(|o| o.settings("a1", "", "low")), json!({"type": "agent.settings", "agentId": "a1", "effort": "low"}));
    }

    #[test]
    fn a_refused_switch_becomes_settings_failed() {
        let f: Frame = serde_json::from_str(r#"{"type":"error","id":"settings","message":"Codex can switch once its first turn has started"}"#).unwrap();
        assert!(matches!(launch_event(&f), Some(Event::SettingsFailed(m)) if m.starts_with("Codex can switch")));
    }

    #[test]
    fn a_finished_tool_keeps_its_use_id_status_output_and_duration() {
        let call = streamed(golden!("server/stream_tool_done.json")).call.unwrap();
        assert_eq!((call.tool_use_id.as_str(), call.name.as_str(), call.status.as_str()), ("t2", "Edit", "ok"));
        assert_eq!((call.output.as_deref(), call.duration_ms), (Some("done"), Some(5)));
    }

    #[test]
    fn an_edit_keeps_its_diff_lines_and_counts() {
        let diff = streamed(golden!("server/stream_tool_done.json")).call.unwrap().detail.diff.unwrap();
        let line = |kind: &str, text: &str| DiffLine { number: Some(1), kind: kind.into(), text: text.into() };
        assert_eq!(diff, FileDiff { lines: vec![line("del", "a"), line("add", "b")], additions: 1, deletions: 1, content_only: false });
    }

    #[test]
    fn a_tasks_item_keeps_its_todos() {
        assert_eq!(streamed(golden!("server/stream_tasks.json")).items, vec![TaskItem { text: "x".into(), status: "in_progress".into() }]);
    }

    #[test]
    fn a_result_keeps_whether_it_succeeded_and_its_output_tokens() {
        let failed = streamed(golden!("server/stream_result.json"));
        assert_eq!((failed.ok, failed.error.as_str()), (false, "interrupted"));
        let f: Frame = serde_json::from_value(golden!("server/timeline.json")).unwrap();
        let done = &f.items[1];
        assert_eq!((done.ok, done.usage.as_ref().map(|u| u.output_tokens)), (true, Some(2)));
    }

    #[test]
    fn a_compact_keeps_its_trigger() {
        assert_eq!(streamed(golden!("server/stream_compact.json")).trigger, "manual");
    }

    #[test]
    fn an_ask_user_question_still_reads_its_questions_from_input() {
        let item: Item = serde_json::from_value(json!({"kind": "tool", "id": "i1", "call": {"toolUseId": "t1", "name": "AskUserQuestion", "status": "running",
            "detail": {"kind": "other", "name": "AskUserQuestion", "input": {"questions": [{"question": "Which color?", "header": "Color", "options": [{"label": "Red"}]}]}}}}))
        .unwrap();
        let detail = item.call.unwrap().detail;
        assert_eq!(detail.questions()[0].question, "Which color?");
        assert_eq!(detail.input["questions"][0]["header"], "Color");
    }

    #[test]
    fn applying_items_bumps_only_that_agents_revision() {
        let mut a = Agents::default();
        let item = |id: &str| Item { id: id.into(), ..Default::default() };
        a.apply(Event::Items("a".into(), vec![item("1")]));
        a.apply(Event::Items("a".into(), vec![item("2")]));
        a.apply(Event::Agents(vec![]));
        assert_eq!((a.revision("a"), a.revision("b")), (2, 0));
    }
}
