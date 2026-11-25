use crate::McpAction;
use crate::automations::AutomationDraft;
use crate::hosts;
use crate::ids::{
    BTW, COMMANDS, CREATE, DELEGATIONS, INTERRUPT, MCP, MCP_ACT, NEXT, PAIR, PROMPT, QUEUE, REMOTE_CONTROL, SETTINGS, auto_id, board_id, host_id, queue_id,
    registry_id, request_id,
};
use crate::locals::Local;
use crate::registry::Shared;
use crate::tickets::{self, TicketDraft};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::mpsc::Sender;

/// A reply to a permission ask. pocketd interrupts the turn on a deny.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Decision {
    Allow,
    Deny,
}

/// Client messages for pocketd. They wait in a queue while it is unreachable.
#[derive(Clone)]
pub struct Outbox(pub(super) Sender<Value>);

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

    /// Hands `text` to pocketd to run now when the agent is idle and to hold in its queue while it works; returns the request id a refusal names.
    pub fn prompt_queued(&self, agent_id: &str, text: &str) -> String {
        let id = format!("{PROMPT}{}", NEXT.fetch_add(1, Ordering::Relaxed));
        self.send(json!({"type": "agent.prompt", "id": id, "agentId": agent_id, "text": text, "queue": true}));
        id
    }

    pub fn queue_list(&self, agent_id: &str) {
        self.send(json!({"type": "queue.list", "id": format!("{QUEUE}{agent_id}"), "agentId": agent_id}));
    }

    pub fn queue_edit(&self, agent_id: &str, entry: &str, text: &str) {
        self.send(json!({"type": "queue.edit", "id": queue_id(), "agentId": agent_id, "entryId": entry, "text": text}));
    }

    pub fn queue_cancel(&self, agent_id: &str, entry: &str) {
        self.send(json!({"type": "queue.cancel", "id": queue_id(), "agentId": agent_id, "entryId": entry}));
    }

    /// Moves `entry` ahead of `before`, or to the end when that is "".
    pub fn queue_reorder(&self, agent_id: &str, entry: &str, before: &str) {
        self.send(json!({"type": "queue.reorder", "id": queue_id(), "agentId": agent_id, "entryId": entry, "before": before}));
    }

    pub fn queue_send_now(&self, agent_id: &str, entry: &str) {
        self.send(json!({"type": "queue.send_now", "id": queue_id(), "agentId": agent_id, "entryId": entry}));
    }

    pub fn interrupt(&self, agent_id: &str) {
        let id = format!("{INTERRUPT}{}", NEXT.fetch_add(1, Ordering::Relaxed));
        self.send(json!({"type": "agent.interrupt", "id": id, "agentId": agent_id}));
    }

    /// Has the agent take the prompts sent into its turn now, rather than after its current step.
    pub fn send_now(&self, agent_id: &str) {
        self.send(json!({"type": "agent.send_now", "id": "send-now", "agentId": agent_id}));
    }

    pub fn compact(&self, agent_id: &str) {
        self.send(json!({"type": "agent.compact", "id": "compact", "agentId": agent_id}));
    }

    /// Asks for the agent's slash commands; a pocketd that can't list them answers with an empty `Commands`.
    pub fn commands(&self, agent_id: &str) {
        self.send(json!({"type": "agent.commands", "id": format!("{COMMANDS}{agent_id}"), "agentId": agent_id}));
    }

    /// Asks for the slash commands `provider` offers in `project` on `account`, "" being the project's own: what
    /// pocketd last found, then what it finds now if that differs.
    pub fn skills(&self, provider: &str, account: &str, project: &str) {
        self.send(json!({"type": "skills.list", "id": "skills", "provider": provider, "account": account, "project": project}));
    }

    /// Asks for every session the chat delegated to.
    pub fn delegations(&self, agent_id: &str) {
        self.send(json!({"type": "agent.delegations", "id": format!("{DELEGATIONS}{agent_id}"), "agentId": agent_id}));
    }

    /// Asks how the agent's MCP servers stand.
    pub fn mcp(&self, agent_id: &str) {
        self.send(json!({"type": "agent.mcp", "id": format!("{MCP}{agent_id}"), "agentId": agent_id}));
    }

    /// Asks the agent to act on MCP server `server`, then how its servers stand.
    pub fn mcp_act(&self, agent_id: &str, action: McpAction, server: &str) {
        self.send(json!({"type": "agent.mcp", "id": format!("{MCP_ACT}{agent_id}"), "agentId": agent_id, "action": action.wire(), "server": server}));
    }

    /// Asks the agent a side question; returns the request id its `Btw` event names.
    pub fn btw(&self, agent_id: &str, question: &str) -> String {
        let id = format!("{BTW}{}", NEXT.fetch_add(1, Ordering::Relaxed));
        self.send(json!({"type": "agent.btw", "id": id, "agentId": agent_id, "text": question}));
        id
    }

    /// Turns Remote Control on or off for the agent's session.
    pub fn remote_control(&self, agent_id: &str, on: bool) {
        self.send(json!({"type": "agent.remote_control", "id": format!("{REMOTE_CONTROL}{agent_id}"), "agentId": agent_id, "enabled": on}));
    }

    /// Switches the agent's model, effort or both; "" keeps one as it is.
    pub fn settings(&self, agent_id: &str, model: &str, effort: &str) {
        let mut msg = json!({"type": "agent.settings", "id": SETTINGS, "agentId": agent_id});
        for (key, value) in [("model", model), ("effort", effort)].into_iter().filter(|(_, v)| !v.is_empty()) {
            msg[key] = value.into();
        }
        self.send(msg);
    }

    /// Switches the model's own setting `id` to `value`.
    pub fn option(&self, agent_id: &str, id: &str, value: &str) {
        self.send(json!({"type": "agent.settings", "id": SETTINGS, "agentId": agent_id, "options": {id: value}}));
    }

    /// Switches what the agent may do without asking.
    pub fn access(&self, agent_id: &str, access: &str) {
        self.send(json!({"type": "agent.settings", "id": SETTINGS, "agentId": agent_id, "access": access}));
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

    /// Lists the folder `path` as a project, with `repo` as its settings; a path already listed stays as it is.
    pub fn project_add(&self, path: &str, repo: Option<&Value>) {
        let mut msg = json!({"type": "project.add", "id": registry_id(), "path": path});
        if let Some(repo) = repo {
            msg["repo"] = repo.clone();
        }
        self.send(msg);
    }

    pub fn project_remove(&self, path: &str) {
        self.send(json!({"type": "project.remove", "id": registry_id(), "path": path}));
    }

    pub fn project_set(&self, path: &str, repo: &Value) {
        self.send(json!({"type": "project.set", "id": registry_id(), "path": path, "repo": repo}));
    }

    pub fn worktree_track(&self, path: &str, tracked: bool) {
        self.send(json!({"type": "worktree.track", "id": registry_id(), "path": path, "tracked": tracked}));
    }

    /// Replaces the shared settings whole.
    pub fn settings_shared(&self, settings: &Shared) {
        self.send(json!({"type": "settings.shared", "id": registry_id(), "settings": settings}));
    }

    /// Adds the ticket to To do, or edits the one with the draft's `id`; `start` also queues it.
    pub fn board_save(&self, draft: TicketDraft, start: bool) {
        self.send(json!({"type": "board.save", "id": board_id(), "ticket": draft, "start": start}));
    }

    pub fn board_delete(&self, ticket_id: &str) {
        self.send(json!({"type": "board.delete", "id": board_id(), "ticketId": ticket_id}));
    }

    pub fn board_start(&self, ticket_id: &str) {
        self.send(json!({"type": "board.start", "id": board_id(), "ticketId": ticket_id}));
    }

    pub fn board_close(&self, ticket_id: &str) {
        self.send(json!({"type": "board.close", "id": board_id(), "ticketId": ticket_id}));
    }

    pub fn board_reopen(&self, ticket_id: &str) {
        self.send(json!({"type": "board.reopen", "id": board_id(), "ticketId": ticket_id}));
    }

    /// Puts the unstarted ticket in `column` before the ticket `before`, or last.
    pub fn board_move(&self, ticket_id: &str, column: tickets::State, before: Option<&str>) {
        self.send(json!({"type": "board.move", "id": board_id(), "ticketId": ticket_id, "column": column, "before": before.unwrap_or_default()}));
    }

    pub fn board_limit(&self, limit: usize) {
        self.send(json!({"type": "board.limit", "id": board_id(), "limit": limit}));
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

    pub fn host_add(&self, target: &hosts::Target) -> String {
        let id = host_id();
        self.send(json!({"type": "host.add", "id": id, "target": target}));
        id
    }

    pub fn host_remove(&self, host_id: &str, uninstall: bool) -> String {
        let id = self::host_id();
        self.send(json!({"type": "host.remove", "id": id, "hostId": host_id, "uninstall": uninstall}));
        id
    }

    pub fn host_retry(&self, host_id: &str) {
        self.send(json!({"type": "host.retry", "id": self::host_id(), "hostId": host_id}));
    }

    pub fn host_install(&self, host_id: &str, update: bool, archive_dir: Option<&str>) -> String {
        let id = self::host_id();
        let mut m = json!({"type": "host.install", "id": id, "hostId": host_id, "update": update});
        if let Some(dir) = archive_dir {
            m["archiveDir"] = dir.into();
        }
        self.send(m);
        id
    }

    pub fn host_sign_in(&self, host_id: &str, password: &str) -> String {
        let id = self::host_id();
        self.send(json!({"type": "host.sign_in", "id": id, "hostId": host_id, "password": password}));
        id
    }

    fn send(&self, m: Value) {
        let _ = self.0.send(m);
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
    use crate::automations;
    use crate::ids::BOARD;
    use serde_json::Value;
    use std::sync::mpsc::channel;

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
            base: String::new(),
            model: String::new(),
            effort: String::new(),
        };
        out.automation_save(draft);
        out.automation_enable("au1", false);
        out.automation_delete("au1");
        out.automation_run("au1");
        let sent: Vec<Value> = rx.try_iter().collect();
        assert_eq!(
            sent[0]["automation"],
            json!({"name": "Morning review", "prompt": "Review new PRs", "provider": "claude", "folder": "/app", "schedule": {"kind": "interval", "everyMin": 60}, "enabled": true})
        );
        let ids: Vec<&str> = sent.iter().map(|m| m["id"].as_str().unwrap()).collect();
        assert!(ids.iter().all(|id| id.starts_with("auto-")) && ids.iter().collect::<std::collections::HashSet<_>>().len() == 4);
        assert_eq!((&sent[1]["type"], &sent[1]["automationId"], &sent[1]["enabled"]), (&json!("automation.enable"), &json!("au1"), &json!(false)));
        assert_eq!((&sent[2]["type"], &sent[2]["automationId"]), (&json!("automation.delete"), &json!("au1")));
        assert_eq!((&sent[3]["type"], &sent[3]["automationId"]), (&json!("automation.run"), &json!("au1")));
    }

    #[test]
    fn board_messages_carry_a_board_request_id() {
        let (tx, rx) = channel();
        let out = Outbox(tx);
        out.board_save(
            tickets::TicketDraft {
                id: None,
                title: "T".into(),
                prompt: "P".into(),
                provider: "claude".into(),
                folder: "/app".into(),
                base: String::new(),
                priority: 0,
                labels: Vec::new(),
            },
            true,
        );
        out.board_start("t1");
        out.board_move("t1", tickets::State::Todo, None);
        out.board_limit(3);
        let sent: Vec<Value> = rx.try_iter().collect();
        assert_eq!(sent[0]["ticket"], json!({"title": "T", "prompt": "P", "provider": "claude", "folder": "/app"}));
        assert_eq!((&sent[0]["type"], &sent[0]["start"]), (&json!("board.save"), &json!(true)));
        assert_eq!((&sent[1]["type"], &sent[1]["ticketId"]), (&json!("board.start"), &json!("t1")));
        assert_eq!((&sent[2]["column"], &sent[2]["before"]), (&json!("todo"), &json!("")));
        assert_eq!((&sent[3]["type"], &sent[3]["limit"]), (&json!("board.limit"), &json!(3)));
        assert!(sent.iter().all(|m| m["id"].as_str().unwrap().starts_with(BOARD)));
    }

    #[test]
    fn registry_messages_carry_a_registry_request_id_and_the_plans_fields() {
        let (tx, rx) = channel();
        let out = Outbox(tx);
        out.project_add("/app", Some(&json!({"name": "App"})));
        out.project_add("/api", None);
        out.project_remove("/app");
        out.project_set("/app", &json!({"setup": "make"}));
        out.worktree_track("/w/fox", false);
        out.settings_shared(&Shared { worktree_root: "/w".into(), ..Shared::default() });
        let sent: Vec<Value> = rx.try_iter().collect();
        assert!(sent.iter().all(|m| m["id"].as_str().unwrap().starts_with("registry-")));
        let sent: Vec<Value> = sent.into_iter().map(without_id).collect();
        assert_eq!(
            sent,
            vec![
                json!({"type": "project.add", "path": "/app", "repo": {"name": "App"}}),
                json!({"type": "project.add", "path": "/api"}),
                json!({"type": "project.remove", "path": "/app"}),
                json!({"type": "project.set", "path": "/app", "repo": {"setup": "make"}}),
                json!({"type": "worktree.track", "path": "/w/fox", "tracked": false}),
                json!({"type": "settings.shared", "settings": {"git": {"remote": "", "commit_all": null, "force_with_lease": false}, "access": "", "worktree_root": "/w", "external_worktrees": false}}),
            ]
        );
    }

    #[test]
    fn prompt_sends_the_golden_agent_prompt_frame_but_its_id() {
        assert_eq!(
            sent_but_id(|o| {
                o.prompt("a1", "hi");
            }),
            without_id(golden!("client/agent_prompt.json"))
        );
    }

    #[test]
    fn interrupt_sends_the_golden_agent_interrupt_frame_but_its_id() {
        assert_eq!(sent_but_id(|o| o.interrupt("a1")), without_id(golden!("client/agent_interrupt.json")));
    }

    #[test]
    fn send_now_sends_the_golden_agent_send_now_frame_but_its_id() {
        assert_eq!(sent_but_id(|o| o.send_now("a1")), without_id(golden!("client/agent_send_now.json")));
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
    fn option_sends_the_golden_agent_settings_options_frame_but_its_id() {
        assert_eq!(sent_but_id(|o| o.option("a1", "fast", "true")), without_id(golden!("client/agent_settings_options.json")));
    }

    #[test]
    fn access_sends_the_golden_agent_access_frame_but_its_id() {
        assert_eq!(sent_but_id(|o| o.access("a1", "edits")), without_id(golden!("client/agent_access.json")));
    }

    #[test]
    fn commands_sends_the_golden_agent_commands_frame_but_its_id() {
        assert_eq!(sent_but_id(|o| o.commands("a1")), without_id(golden!("client/agent_commands.json")));
    }

    #[test]
    fn skills_sends_the_golden_skills_list_frame_but_its_id() {
        assert_eq!(sent_but_id(|o| o.skills("codex", "3f9a0c1b2d4e", "/Users/me/dev/pocket")), without_id(golden!("client/skills_list_account.json")));
    }

    #[test]
    fn mcp_sends_the_golden_agent_mcp_frame_but_its_id() {
        assert_eq!(sent_but_id(|o| o.mcp("a1")), without_id(golden!("client/agent_mcp.json")));
    }

    #[test]
    fn mcp_act_sends_the_golden_sign_in_frame_but_its_id() {
        assert_eq!(sent_but_id(|o| o.mcp_act("a1", McpAction::Auth, "linear")), without_id(golden!("client/agent_mcp_auth.json")));
    }

    #[test]
    fn remote_control_sends_the_golden_frame_but_its_id() {
        assert_eq!(sent_but_id(|o| o.remote_control("a1", true)), without_id(golden!("client/agent_remote_control.json")));
    }
}
