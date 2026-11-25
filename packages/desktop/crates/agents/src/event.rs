use crate::automations::{self, Automation, Run};
use crate::delegations::Delegation;
use crate::hosts;
use crate::ids::{AUTO, BOARD, BTW, COMMANDS, CREATE, MCP, MCP_ACT, REGISTRY, REMOTE_CONTROL, SETTINGS};
use crate::locals::Local;
use crate::providers::ProviderInfo;
use crate::queue::Entry;
use crate::registry;
use crate::resend::PromptState;
use crate::tickets::{self, Ticket};
use crate::{Command, Item, McpServer, Permission, Summary};
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize, Default, Debug)]
#[serde(default, rename_all = "camelCase")]
pub(super) struct Frame {
    #[serde(rename = "type")]
    pub(super) kind: String,
    pub(super) agent_id: String,
    pub(super) agents: Vec<Summary>,
    pub(super) agent: Option<Summary>,
    pub(super) item: Option<Item>,
    pub(super) items: Vec<Item>,
    pub(super) max_seq: i64,
    pub(super) has_older: bool,
    pub(super) request: Option<Permission>,
    pub(super) request_id: String,
    pub(super) host: Option<Host>,
    pub(super) scopes: Vec<String>,
    pub(super) caps: Vec<String>,
    pub(super) id: String,
    pub(super) message: String,
    pub(super) url: String,
    pub(super) code: String,
    pub(super) expires_at: i64,
    pub(super) name: String,
    pub(super) terminal_id: String,
    pub(super) cwd: String,
    pub(super) setup: bool,
    pub(super) detail: String,
    pub(super) providers: Vec<ProviderInfo>,
    pub(super) step: String,
    pub(super) note: String,
    pub(super) names: HashMap<String, String>,
    pub(super) locals: Vec<Local>,
    pub(super) commands: Vec<Command>,
    pub(super) provider: String,
    pub(super) project: String,
    pub(super) account: String,
    pub(super) servers: Vec<McpServer>,
    pub(super) auth_url: String,
    pub(super) answer: String,
    pub(super) version: String,
    #[serde(deserialize_with = "automations::known")]
    pub(super) automations: Vec<Automation>,
    pub(super) runs: Vec<Run>,
    #[serde(deserialize_with = "automations::readable_items")]
    pub(super) tickets: Vec<Ticket>,
    pub(super) limit: Option<usize>,
    pub(super) state: String,
    pub(super) error: String,
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
pub(super) fn host_event(f: &Frame) -> Option<Event> {
    matches!(f.kind.as_str(), "hello.ok" | "host.changed").then(|| f.host.clone().map(Event::Host)).flatten()
}

pub enum Event {
    Agents(Vec<Summary>),
    Agent(Box<Summary>),
    Items(String, Vec<Item>),
    /// The newest of a chat's items, which the link asks pocketd for once the chat is opened or on screen; `max_seq` is the newest seq it had then.
    Page {
        agent: String,
        items: Vec<Item>,
        max_seq: i64,
    },
    Asked(Permission),
    Resolved(String),
    /// hello.ok arrived with the scopes pocketd granted this conn, the caps both sides speak and pocketd's build.
    Connected {
        scopes: Vec<String>,
        caps: Vec<String>,
        version: String,
    },
    Host(Host),
    /// A pairing code from `Outbox::pair_begin`; `expires_at` is Unix ms.
    PairCode {
        url: String,
        code: String,
        expires_at: i64,
    },
    /// The phone redeemed the code; carries the device's name.
    Paired(String),
    PairFailed(String),
    /// pocketd made the Terminal for a create; `setup` runs before the agent.
    Creating {
        request: String,
        terminal: String,
        cwd: String,
        setup: bool,
    },
    /// The step a create has reached; a step sent again carries a note on how it went.
    Progress {
        request: String,
        step: String,
        note: String,
    },
    Created {
        request: String,
        agent: String,
    },
    CreateFailed {
        request: String,
        code: String,
        message: String,
        detail: String,
    },
    Providers {
        providers: Vec<ProviderInfo>,
    },
    /// pocketd couldn't switch an agent's model or effort.
    SettingsFailed(String),
    /// Every worktree's display name by path, sent whole.
    Names(HashMap<String, String>),
    /// pocketd couldn't name the session from its prompt.
    NamingFailed(String),
    /// Every Local the user added, sent whole.
    Locals(Vec<Local>),
    /// The projects, tracked worktrees and shared settings the pocketd owns, sent whole.
    Registry(registry::List),
    /// pocketd refused a change to them; the list it sends next undoes it.
    RegistryFailed(String),
    /// Every automation and run, sent whole.
    Automations {
        automations: Vec<Automation>,
        runs: Vec<Run>,
    },
    /// pocketd refused an automation message.
    AutomationError(String),
    /// Every ticket and the limit, sent whole.
    Board {
        tickets: Vec<Ticket>,
        limit: usize,
    },
    /// pocketd refused a board message.
    BoardError(String),
    /// pocketd refused the prompt sent under `request`, the agent didn't take it, or the link stayed down; `reason` says
    /// why. It isn't sent again, so it can't be typed twice.
    PromptFailed {
        agent: String,
        request: String,
        reason: String,
    },
    /// Where the prompt sent under `request` stands now.
    PromptState {
        agent: String,
        request: String,
        state: PromptState,
    },
    /// pocketd refused to stop the agent's turn, or the socket never took the stop; `reason` says why.
    InterruptFailed {
        agent: String,
        reason: String,
    },
    /// The slash commands the agent offers; empty when pocketd couldn't list them.
    Commands {
        agent: String,
        commands: Vec<Command>,
    },
    /// The slash commands `provider` offers in `project` on `account`, read before any agent runs there.
    Skills {
        provider: String,
        account: String,
        project: String,
        commands: Vec<Command>,
    },
    /// The answer to the side question `request` asked, or why there's none.
    Btw {
        request: String,
        answer: Result<String, String>,
    },
    /// Remote Control turned on, with the link the agent's session continues at, off with none, or why it didn't.
    RemoteControl {
        agent: String,
        url: Result<String, String>,
    },
    /// How the agent's MCP servers stand, or why pocketd couldn't tell; after a sign-in starts, the URL the user
    /// finishes it at.
    /// `acted` when it answers an action rather than a plain ask, as replies can cross.
    Mcp {
        agent: String,
        servers: Result<Vec<McpServer>, String>,
        auth_url: String,
        acted: bool,
    },
    /// Every session `agent` delegated to, sent whole.
    Delegations {
        agent: String,
        items: Vec<Delegation>,
    },
    /// One of `agent`'s delegations changed.
    Delegated {
        agent: String,
        delegation: Delegation,
    },
    Queue {
        agent: String,
        entries: Vec<Entry>,
    },
    /// pocketd refused a queue change or a queued send, or the socket never took it; `text` is what the user had typed for it, if anything.
    QueueFailed {
        agent: String,
        reason: String,
        text: Option<String>,
    },
    McpActFailed {
        agent: String,
        reason: String,
    },
    Hosts(Vec<hosts::Entry>),
    HostUpdate {
        request: String,
        host: hosts::Entry,
    },
    HostProgress {
        host: String,
        update: bool,
        progress: hosts::Progress,
    },
    HostError {
        request: String,
        code: String,
        detail: Option<hosts::Failure>,
        message: String,
    },
    HostDone(String),
}

pub(super) fn launch_event(f: &Frame) -> Option<Event> {
    Some(match f.kind.as_str() {
        "agent.creating" => Event::Creating { request: f.request_id.clone(), terminal: f.terminal_id.clone(), cwd: f.cwd.clone(), setup: f.setup },
        "agent.progress" => Event::Progress { request: f.request_id.clone(), step: f.step.clone(), note: f.note.clone() },
        "agent.created" => Event::Created { request: f.request_id.clone(), agent: f.agent_id.clone() },
        "agent.providers" => Event::Providers { providers: f.providers.clone() },
        "error" if f.id == SETTINGS => Event::SettingsFailed(f.message.clone()),
        "error" => Event::CreateFailed {
            request: f.id.strip_prefix(CREATE)?.to_string(),
            code: f.code.clone(),
            message: readable(&f.message),
            detail: f.detail.clone(),
        },
        _ => return None,
    })
}

/// A pocketd older than this app rejects a create it can't parse as malformed.
pub const OUTDATED: &str = "Anywhere's background service is out of date. Restart it to update.";

fn readable(message: &str) -> String {
    if message == "Malformed message" { OUTDATED.into() } else { message.into() }
}

pub(super) fn names_event(f: &Frame) -> Option<Event> {
    match f.kind.as_str() {
        "worktree.names" => Some(Event::Names(f.names.clone())),
        "naming.failed" => Some(Event::NamingFailed(f.agent_id.clone())),
        "local.list" => Some(Event::Locals(f.locals.clone())),
        "agent.commands" => Some(Event::Commands { agent: f.agent_id.clone(), commands: f.commands.clone() }),
        "skills.list" => {
            Some(Event::Skills { provider: f.provider.clone(), account: f.account.clone(), project: f.project.clone(), commands: f.commands.clone() })
        }
        "agent.btw" => Some(Event::Btw { request: f.id.clone(), answer: Ok(f.answer.clone()) }),
        "error" if f.id.starts_with(BTW) => Some(Event::Btw { request: f.id.clone(), answer: Err(readable(&f.message)) }),
        "agent.remote_control" => Some(Event::RemoteControl { agent: f.agent_id.clone(), url: Ok(f.url.clone()) }),
        "error" if f.id.starts_with(REMOTE_CONTROL) => {
            Some(Event::RemoteControl { agent: f.id[REMOTE_CONTROL.len()..].to_string(), url: Err(readable(&f.message)) })
        }
        "agent.mcp" => {
            Some(Event::Mcp { agent: f.agent_id.clone(), servers: Ok(f.servers.clone()), auth_url: f.auth_url.clone(), acted: f.id.starts_with(MCP_ACT) })
        }
        "error" if f.id.starts_with(MCP) => {
            Some(Event::Mcp { agent: f.id[MCP.len()..].to_string(), servers: Err(readable(&f.message)), auth_url: String::new(), acted: false })
        }
        "error" if f.id.starts_with(MCP_ACT) => Some(Event::McpActFailed { agent: f.id[MCP_ACT.len()..].to_string(), reason: readable(&f.message) }),
        "error" => Some(Event::Commands { agent: f.id.strip_prefix(COMMANDS)?.to_string(), commands: Vec::new() }),
        _ => None,
    }
}

pub(super) fn registry_event(f: &Frame, raw: &str) -> Option<Event> {
    match f.kind.as_str() {
        "error" if f.id.starts_with(REGISTRY) => Some(Event::RegistryFailed(readable(&f.message))),
        kind => registry::decode(kind, raw).map(Event::Registry),
    }
}

pub(super) fn automation_event(f: &Frame) -> Option<Event> {
    match f.kind.as_str() {
        "automations" => Some(Event::Automations { automations: f.automations.clone(), runs: f.runs.clone() }),
        "error" if f.id.starts_with(AUTO) => Some(Event::AutomationError(readable(&f.message))),
        _ => None,
    }
}

/// The delegation frames, read apart from `Frame`, whose `items` are timeline items.
#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct DelegationFrame {
    agent_id: String,
    items: Vec<Delegation>,
    delegation: Option<Delegation>,
}

pub(super) fn delegation_event(kind: &str, raw: &str) -> Option<Event> {
    if !matches!(kind, "agent.delegations" | "delegation.update") {
        return None;
    }
    let f: DelegationFrame = serde_json::from_str(raw).ok()?;
    match f.delegation {
        Some(delegation) => Some(Event::Delegated { agent: f.agent_id, delegation }),
        None if kind == "agent.delegations" => Some(Event::Delegations { agent: f.agent_id, items: f.items }),
        None => None,
    }
}

pub(super) fn board_event(f: &Frame) -> Option<Event> {
    match f.kind.as_str() {
        "board" => Some(Event::Board { tickets: f.tickets.clone(), limit: f.limit.unwrap_or(tickets::DEFAULT_LIMIT) }),
        "error" if f.id.starts_with(BOARD) => Some(Event::BoardError(readable(&f.message))),
        _ => None,
    }
}

/// `events` with each run of items for the same chat as one, so a burst of streamed drafts redraws once.
pub fn coalesce(events: Vec<Event>) -> Vec<Event> {
    let mut out: Vec<Event> = Vec::with_capacity(events.len());
    for ev in events {
        match (out.last_mut(), ev) {
            (Some(Event::Items(last, items)), Event::Items(id, more)) if *last == id => items.extend(more),
            (_, ev) => out.push(ev),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Agents, McpStatus, providers};
    use serde_json::Value;

    #[test]
    fn a_delegations_reply_and_an_update_decode_into_their_chat() {
        let Some(Event::Delegations { agent, items }) = delegation_event("agent.delegations", golden_text!("server/agent_delegations.json")) else {
            panic!("no delegations")
        };
        assert_eq!((agent.as_str(), items.len(), items[0].child_id.as_str()), ("a1", 2, "a2"));
        let Some(Event::Delegated { agent, delegation }) = delegation_event("delegation.update", golden_text!("server/delegation_update.json")) else {
            panic!("no update")
        };
        assert_eq!((agent.as_str(), delegation.question.as_str()), ("a1", "Which test runner?"));
        assert!(delegation_event("agent.list", "{}").is_none());
    }

    #[test]
    fn a_child_summary_and_a_delivery_item_decode_and_unknown_fields_stay_harmless() {
        let f: Frame = serde_json::from_str(golden_text!("server/agent_update_delegated.json")).unwrap();
        let a = f.agent.unwrap();
        assert_eq!((a.parent.as_str(), a.relationship.as_str()), ("a1", "subagent"));
        let f: Frame = serde_json::from_str(golden_text!("server/stream_delivery.json")).unwrap();
        let i = f.item.unwrap();
        assert_eq!(
            (i.kind.as_str(), i.titles, i.child_ids, i.agents, i.outcome.as_str()),
            ("delivery", vec!["Write the tests".to_string()], vec!["a2".to_string()], vec!["codex".to_string()], "done")
        );
    }

    #[test]
    fn a_burst_of_items_for_one_chat_lands_as_one_event_with_the_last_draft_winning() {
        let item = |id: &str, seq, text: &str| Item { id: id.into(), seq, kind: "assistant".into(), text: text.into(), ..Default::default() };
        let items = |id: &str, it: Item| Event::Items(id.into(), vec![it]);
        let events =
            coalesce(vec![items("a", item("1", 1, "h")), items("a", item("1", 1, "hi")), items("b", item("1", 1, "x")), items("a", item("2", 2, "yo"))]);
        assert_eq!(events.len(), 3, "a different chat between keeps the order");
        let mut a = Agents::default();
        events.into_iter().for_each(|ev| a.apply(ev));
        let t: Vec<_> = a.timelines["a"].iter().map(|i| i.text.as_str()).collect();
        assert_eq!(t, ["hi", "yo"]);
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
    fn launch_replies_become_launch_events() {
        let ev = |raw: &str| launch_event(&serde_json::from_str::<Frame>(raw).unwrap());
        let creating = include_str!("../../../../pocketd/internal/proto/testdata/golden/server/agent_creating.json");
        let progress = include_str!("../../../../pocketd/internal/proto/testdata/golden/server/agent_progress.json");
        let created = include_str!("../../../../pocketd/internal/proto/testdata/golden/server/agent_created.json");
        let providers = include_str!("../../../../pocketd/internal/proto/testdata/golden/server/agent_providers.json");
        let failed = include_str!("../../../../pocketd/internal/proto/testdata/golden/server/error_detail.json");
        assert!(
            matches!(ev(creating), Some(Event::Creating { request, terminal, cwd, setup: true }) if request == "r1" && terminal == "t1" && cwd == "/w/fix")
        );
        assert!(matches!(ev(progress), Some(Event::Progress { request, step, note })
            if request == "r1" && step == "fetch" && note == "Couldn't fetch, using local main"));
        assert!(matches!(ev(created), Some(Event::Created { request, agent }) if request == "r1" && agent == "a1"));
        let Some(Event::Providers { providers }) = ev(providers) else { panic!("not providers") };
        assert_eq!(providers[0].chat, Some(providers::ChatCaps { interrupt: true, compact: true, send_while_working: true, sub_agents: true, send_now: true }));
        assert_eq!(
            providers[1].chat,
            Some(providers::ChatCaps { interrupt: true, compact: true, send_while_working: true, sub_agents: false, send_now: false })
        );
        assert!(providers[1].efforts.is_empty());
        assert!(matches!(ev(failed), Some(Event::CreateFailed { request, code, message, detail })
            if request == "r1" && code == "spawn_failed" && message == "Setup exited 1" && detail == "npm ERR! missing script: setup"));
        assert!(ev(r#"{"type":"error","id":"pair","message":"x"}"#).is_none());
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
    fn a_host_frame_carries_a_failed_upgrade() {
        let frame = serde_json::from_str::<Frame>(r#"{"type":"host.changed","host":{"tailnet":true,"keepingAwake":false,"upgradeFailed":"1.2.0"}}"#).unwrap();
        let Some(Event::Host(host)) = host_event(&frame) else { panic!("no host event") };
        assert_eq!(host.upgrade_failed, "1.2.0");
    }

    #[test]
    fn registry_frames_become_registry_events() {
        let ev = |raw: &str| registry_event(&serde_json::from_str::<Frame>(raw).unwrap(), raw);
        assert!(
            matches!(ev(r#"{"type":"registry.list","projects":[{"id":"p1","path":"/app","repo":{}}],"tracked":[],"settings":{}}"#), Some(Event::Registry(l)) if l.projects[0].path == "/app")
        );
        assert!(
            matches!(ev(r#"{"type":"error","id":"registry-3","code":"unknown_project","message":"No such project"}"#), Some(Event::RegistryFailed(m)) if m == "No such project")
        );
        assert!(ev(r#"{"type":"error","id":"board-3","message":"Nope"}"#).is_none());
    }

    #[test]
    fn board_frames_become_board_events() {
        let ev = |raw: &str| board_event(&serde_json::from_str::<Frame>(raw).unwrap());
        let snapshot = include_str!("../../../../pocketd/internal/proto/testdata/golden/server/board.json");
        assert!(
            matches!(ev(snapshot), Some(Event::Board { ref tickets, limit: 4 }) if (tickets[0].number, tickets[0].priority, tickets[0].labels.as_slice()) == (7, 2, ["Bug".to_string()].as_slice()) && (tickets[1].number, tickets[1].priority) == (0, 0))
        );
        assert!(
            matches!(ev(snapshot), Some(Event::Board { tickets, limit: 4 }) if tickets.len() == 2 && tickets[0].status == tickets::Status::NeedsYou && tickets[1].closed_why == tickets::ClosedWhy::Merged)
        );
        let newer = r#"{"type":"board","limit":2,"tickets":[{"id":"t2","state":"nonsense"}]}"#;
        assert!(matches!(ev(newer), Some(Event::Board { tickets, limit: 2 }) if tickets.is_empty()));
        assert!(matches!(ev(r#"{"type":"error","id":"board-3","message":"Nope"}"#), Some(Event::BoardError(m)) if m == "Nope"));
        assert!(ev(r#"{"type":"error","id":"auto-3","message":"Nope"}"#).is_none());
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
    fn the_golden_commands_list_becomes_commands_for_its_agent() {
        let f: Frame = serde_json::from_value(golden!("server/agent_commands.json")).unwrap();
        let Some(Event::Commands { agent, commands }) = names_event(&f) else { panic!("no commands") };
        assert_eq!((agent.as_str(), commands.len()), ("a1", 2));
        assert_eq!((commands[1].name.as_str(), commands[1].argument_hint.as_str()), ("review", "<pr>"));
    }

    #[test]
    fn the_golden_skills_list_becomes_skills_for_its_provider_account_and_project() {
        let f: Frame = serde_json::from_value(golden!("server/skills_list.json")).unwrap();
        let Some(Event::Skills { provider, account, project, commands }) = names_event(&f) else { panic!("no skills") };
        assert_eq!((provider.as_str(), account.as_str(), project.as_str()), ("claude", "3f9a0c1b2d4e", "/Users/me/dev/pocket"));
        assert_eq!((commands[1].name.as_str(), commands[1].argument_hint.as_str()), ("skill-creator:skill-creator", "<skill>"));
    }

    #[test]
    fn the_golden_mcp_list_or_its_refusal_reaches_its_agent() {
        let f: Frame = serde_json::from_value(golden!("server/agent_mcp.json")).unwrap();
        let Some(Event::Mcp { agent, servers: Ok(servers), auth_url, acted: false }) = names_event(&f) else { panic!("no servers") };
        assert_eq!((agent.as_str(), auth_url.as_str()), ("a1", ""));
        assert_eq!(servers[1], McpServer { name: "claude.ai Linear".into(), status: McpStatus::NeedsAuth, scope: "claude.ai".into() });
        let f: Frame = serde_json::from_str(r#"{"type":"error","id":"mcp-a1","message":"Malformed message"}"#).unwrap();
        assert!(matches!(names_event(&f), Some(Event::Mcp { agent, servers: Err(e), .. }) if agent == "a1" && e == OUTDATED));
    }

    #[test]
    fn a_sign_in_brings_its_url_and_a_refused_action_its_reason() {
        let f: Frame = serde_json::from_value(golden!("server/agent_mcp_auth.json")).unwrap();
        assert!(matches!(names_event(&f), Some(Event::Mcp { auth_url, .. }) if auth_url == "https://mcp.linear.app/authorize?state=s1"));
        let f: Frame = serde_json::from_str(r#"{"type":"agent.mcp","id":"mcpact-a1","agentId":"a1","servers":[]}"#).unwrap();
        assert!(matches!(names_event(&f), Some(Event::Mcp { acted: true, .. })), "an action's reply says it is one");
        let f: Frame = serde_json::from_str(r#"{"type":"error","id":"mcpact-a1","message":"Server not found: x"}"#).unwrap();
        assert!(matches!(names_event(&f), Some(Event::McpActFailed { agent, reason }) if agent == "a1" && reason == "Server not found: x"));
    }

    #[test]
    fn a_side_answer_or_its_refusal_reaches_its_agent() {
        let f: Frame = serde_json::from_value(golden!("server/agent_btw.json")).unwrap();
        assert!(matches!(names_event(&f), Some(Event::Btw { request, answer: Ok(a) }) if request == "b1" && a == "It reads the config once at start."));
        let f: Frame = serde_json::from_str(r#"{"type":"error","id":"btw-7","message":"The agent had no answer"}"#).unwrap();
        assert!(matches!(names_event(&f), Some(Event::Btw { request, answer: Err(e) }) if request == "btw-7" && e == "The agent had no answer"));
    }

    #[test]
    fn remote_control_brings_its_link_none_once_off_or_why_it_failed() {
        let f: Frame = serde_json::from_value(golden!("server/agent_remote_control.json")).unwrap();
        assert!(
            matches!(names_event(&f), Some(Event::RemoteControl { agent, url: Ok(u) }) if agent == "a1" && u == "https://claude.ai/code/session_01La8mPYSeyy7PpSsj7a8Yjq")
        );
        let f: Frame = serde_json::from_value(golden!("server/agent_remote_control_off.json")).unwrap();
        assert!(matches!(names_event(&f), Some(Event::RemoteControl { url: Ok(u), .. }) if u.is_empty()));
        let f: Frame =
            serde_json::from_str(r#"{"type":"error","id":"remotecontrol-a1","message":"Remote Control requires a claude.ai subscription"}"#).unwrap();
        assert!(
            matches!(names_event(&f), Some(Event::RemoteControl { agent, url: Err(e) }) if agent == "a1" && e == "Remote Control requires a claude.ai subscription")
        );
    }

    #[test]
    fn a_refused_commands_list_becomes_an_empty_one() {
        let f: Frame = serde_json::from_str(r#"{"type":"error","id":"commands-a1","message":"Malformed message"}"#).unwrap();
        assert!(matches!(names_event(&f), Some(Event::Commands { agent, commands }) if agent == "a1" && commands.is_empty()));
    }

    #[test]
    fn a_refused_switch_becomes_settings_failed() {
        let f: Frame = serde_json::from_str(r#"{"type":"error","id":"settings","message":"Codex can switch once its first turn has started"}"#).unwrap();
        assert!(matches!(launch_event(&f), Some(Event::SettingsFailed(m)) if m.starts_with("Codex can switch")));
    }
}
