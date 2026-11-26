mod card;
mod popover;
mod question;

use crate::desktop::Desktop;
use crate::desktop::chrome::Layout;
use crate::status::{self, Status};
use crate::util::basename;
use agents::{Agents, Decision, Permission, Question, Summary};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::*;
pub(crate) use popover::hung;
use question::Wizard;
use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};
use store::prefs::sidebar::InboxSort;

pub(crate) fn noted(a: &Summary) -> Option<Status> {
    crate::status::alert_status(a).filter(Status::alerting)
}

pub fn count(agents: &Agents) -> usize {
    agents.list.iter().filter(|a| noted(a).is_some()).count()
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Tab {
    NeedsYou,
    Updates,
}

impl Tab {
    fn of(status: Status) -> Self {
        if status == Status::Done { Tab::Updates } else { Tab::NeedsYou }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub(crate) struct Card {
    pub(crate) agent: String,
    pub(crate) status: Status,
    pub(crate) project: String,
    pub(crate) mark: ui::Mark,
    pub(crate) provider: String,
    pub(crate) agent_name: String,
    pub(crate) title: String,
    pub(crate) delegated_by: Option<String>,
    pub(crate) at: i64,
    pub(crate) body: Body,
}

#[derive(Clone, PartialEq, Debug)]
pub(crate) enum Body {
    Asks(Vec<Permission>),
    Said { text: String, chat: bool },
    Failed { error: String, retry: bool },
    Done,
}

pub(crate) fn body(status: Status, asks: Vec<Permission>, said: &str, chat: bool, error: &str, retry: bool) -> Body {
    match status {
        Status::NeedsYou if !asks.is_empty() => Body::Asks(asks),
        Status::NeedsYou => Body::Said { text: said.trim().to_string(), chat },
        Status::Failed => Body::Failed { error: error.trim().to_string(), retry },
        _ => Body::Done,
    }
}

pub(crate) fn order(cards: &mut [Card], sort: InboxSort) {
    match sort {
        InboxSort::Urgent => cards.sort_by_key(|c| (c.status, c.at)),
        InboxSort::Newest => cards.sort_by_key(|c| Reverse(c.at)),
        InboxSort::Oldest => cards.sort_by_key(|c| c.at),
    }
}

fn opening_tab(cards: &[Card]) -> Tab {
    if !cards.iter().any(|c| Tab::of(c.status) == Tab::NeedsYou) && cards.iter().any(|c| Tab::of(c.status) == Tab::Updates) {
        Tab::Updates
    } else {
        Tab::NeedsYou
    }
}

fn mark_all_seen(cards: &[Card]) -> Vec<String> {
    cards.iter().filter(|c| Tab::of(c.status) == Tab::Updates).map(|c| c.agent.clone()).collect()
}

#[derive(Clone, PartialEq, Debug)]
pub(crate) enum Replying {
    Deny(String),
    Other(String),
    Prompt(String),
}

/// What the cards were told, kept until pocketd's next word drops the ask or the session.
#[derive(Default)]
pub(crate) struct Answers {
    sent: HashMap<String, String>,
    wizards: HashMap<String, Wizard>,
    expanded: HashSet<String>,
}

impl Answers {
    pub(crate) fn shown<'a>(&self, asks: &'a [Permission]) -> Option<(usize, &'a Permission)> {
        asks.iter().enumerate().find(|(_, p)| !self.sent.contains_key(&p.request_id))
    }

    pub(crate) fn settled(&self, card: &Card) -> Option<&str> {
        match &card.body {
            Body::Asks(asks) if self.shown(asks).is_none() => asks.last().and_then(|p| self.sent.get(&p.request_id)).map(String::as_str),
            Body::Asks(_) => None,
            _ => self.sent.get(&card.agent).map(String::as_str),
        }
    }

    /// Records `key` as answered; false when it already was, so nothing goes out twice.
    fn send(&mut self, key: &str, line: String) -> bool {
        if self.sent.contains_key(key) {
            return false;
        }
        self.sent.insert(key.to_string(), line);
        true
    }

    fn forget_gone(&mut self, cards: &[Card]) {
        let live: HashSet<&str> = cards
            .iter()
            .flat_map(|c| {
                let asks = match &c.body {
                    Body::Asks(asks) => asks.iter().map(|p| p.request_id.as_str()).collect(),
                    _ => Vec::new(),
                };
                asks.into_iter().chain(std::iter::once(c.agent.as_str()))
            })
            .collect();
        self.sent.retain(|k, _| live.contains(k.as_str()));
        self.wizards.retain(|k, _| live.contains(k.as_str()));
        self.expanded.retain(|k| live.contains(k.as_str()));
    }

    pub(crate) fn wizard(&self, request: &str) -> Option<&Wizard> {
        self.wizards.get(request)
    }

    pub(crate) fn expanded(&self, request: &str) -> bool {
        self.expanded.contains(request)
    }
}

pub(crate) enum InboxEvent {
    Open(String),
    Decide { request: String, decision: Decision },
    Reply { request: String, text: String },
    Answer { request: String, answers: HashMap<String, String> },
    Prompt { agent: String, text: String },
    Retry(String),
    Seen(Vec<String>),
}

pub struct Inbox {
    pub(crate) open: bool,
    tab: Tab,
    cards: Vec<Card>,
    observe_only: bool,
    answers: Answers,
    replying: Option<Replying>,
    reply: Entity<InputState>,
    _sub: Subscription,
}

impl EventEmitter<InboxEvent> for Inbox {}

impl Inbox {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let reply = cx.new(|cx| InputState::new(window, cx));
        let _sub = cx.subscribe_in(&reply, window, |this, _, e: &InputEvent, window, cx| {
            if let InputEvent::PressEnter { .. } = e {
                this.send_reply(window, cx);
            }
        });
        Self { open: false, tab: Tab::NeedsYou, cards: Vec::new(), observe_only: false, answers: Answers::default(), replying: None, reply, _sub }
    }

    pub(crate) fn show(&mut self, cards: Vec<Card>, observe_only: bool, cx: &mut Context<Self>) {
        (self.open, self.tab, self.replying) = (true, opening_tab(&cards), None);
        self.set(cards, observe_only, cx);
    }

    pub(crate) fn set(&mut self, cards: Vec<Card>, observe_only: bool, cx: &mut Context<Self>) {
        if self.cards != cards || self.observe_only != observe_only {
            self.answers.forget_gone(&cards);
            (self.cards, self.observe_only) = (cards, observe_only);
            cx.notify();
        }
    }

    pub(crate) fn show_tab(&mut self, tab: Tab, cx: &mut Context<Self>) {
        self.tab = tab;
        cx.notify();
    }

    pub(crate) fn shut(&mut self, cx: &mut Context<Self>) {
        if self.open {
            (self.open, self.replying) = (false, None);
            cx.notify();
        }
    }

    pub(crate) fn hung_from(this: &Entity<Self>, layout: Layout, bell: Layout, cx: &App) -> Option<Entity<Self>> {
        (this.read(cx).open && layout == bell).then(|| this.clone())
    }

    fn card(&self, agent: &str) -> Option<&Card> {
        self.cards.iter().find(|c| c.agent == agent)
    }

    fn decide(&mut self, request: &str, decision: Decision, cx: &mut Context<Self>) {
        let line = if decision == Decision::Allow { "Allowed once · agent resumed" } else { "Denied · agent stopped that step" };
        if self.answers.send(request, line.into()) {
            cx.emit(InboxEvent::Decide { request: request.to_string(), decision });
        }
        cx.notify();
    }

    fn retry(&mut self, agent: &str, cx: &mut Context<Self>) {
        if self.answers.send(agent, "Retrying · back to Working".into()) {
            cx.emit(InboxEvent::Retry(agent.to_string()));
        }
        cx.notify();
    }

    fn expand(&mut self, request: &str, cx: &mut Context<Self>) {
        self.answers.expanded.insert(request.to_string());
        cx.notify();
    }

    fn start_reply(&mut self, replying: Replying, placeholder: String, window: &mut Window, cx: &mut Context<Self>) {
        self.replying = Some(replying);
        self.reply.update(cx, |s, cx| {
            s.set_value("", window, cx);
            s.set_placeholder(placeholder, window, cx);
            s.focus(window, cx);
        });
        cx.notify();
    }

    fn cancel_reply(&mut self, cx: &mut Context<Self>) {
        self.replying = None;
        cx.notify();
    }

    fn send_reply(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.reply.read(cx).value().trim().to_string();
        let Some(replying) = self.replying.clone().filter(|_| !text.is_empty()) else { return };
        self.replying = None;
        self.reply.update(cx, |s, cx| s.set_value("", window, cx));
        match replying {
            Replying::Deny(request) => {
                if self.answers.send(&request, "Denied with your note".into()) {
                    cx.emit(InboxEvent::Reply { request, text });
                }
            }
            Replying::Other(request) => {
                if let Some(qs) = self.questions(&request) {
                    let w = self.answers.wizards.entry(request.clone()).or_insert_with(|| Wizard::new(qs.len()));
                    w.type_answer(text);
                    self.next_page(&request, cx);
                }
            }
            Replying::Prompt(agent) => {
                let line = format!("Sent to {}", self.card(&agent).map(|c| c.agent_name.as_str()).unwrap_or("the agent"));
                if self.answers.send(&agent, line) {
                    cx.emit(InboxEvent::Prompt { agent, text });
                }
            }
        }
        cx.notify();
    }

    fn questions(&self, request: &str) -> Option<Vec<Question>> {
        self.cards.iter().find_map(|c| match &c.body {
            Body::Asks(asks) => asks.iter().find(|p| p.request_id == request).map(Permission::questions),
            _ => None,
        })
    }

    fn pick(&mut self, request: &str, option: usize, cx: &mut Context<Self>) {
        let Some(qs) = self.questions(request) else { return };
        let w = self.answers.wizards.entry(request.to_string()).or_insert_with(|| Wizard::new(qs.len()));
        let page = w.page();
        if w.pick(&qs, option) {
            let request = request.to_string();
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(crate::chat::question::AUTO_ADVANCE).await;
                this.update(cx, |this, cx| {
                    if this.answers.wizards.get(&request).is_some_and(|w| w.still_picked(page, option)) {
                        this.next_page(&request, cx);
                    }
                })
                .ok();
            })
            .detach();
        }
        cx.notify();
    }

    fn next_page(&mut self, request: &str, cx: &mut Context<Self>) {
        let Some(qs) = self.questions(request) else { return };
        let Some(w) = self.answers.wizards.get_mut(request) else { return };
        if let Some((answers, line)) = w.next(&qs)
            && self.answers.send(request, line)
        {
            cx.emit(InboxEvent::Answer { request: request.to_string(), answers });
        }
        cx.notify();
    }

    fn back(&mut self, request: &str, cx: &mut Context<Self>) {
        if let Some(w) = self.answers.wizards.get_mut(request) {
            w.back();
        }
        cx.notify();
    }
}

impl Desktop {
    pub(crate) fn inbox(&self, cx: &App) -> Entity<Inbox> {
        self.sidebar.read(cx).inbox.clone()
    }

    pub(crate) fn toggle_inbox(&mut self, cx: &mut Context<Self>) {
        let inbox = self.inbox(cx);
        let open = !inbox.read(cx).open;
        self.close_menus(cx);
        if open {
            let (cards, observe_only) = (self.inbox_cards(cx), self.agents.observe_only());
            inbox.update(cx, |i, cx| i.show(cards, observe_only, cx));
        }
        cx.notify();
    }

    /// Focus shows no bell to hang the inbox from, so it shuts there.
    pub(crate) fn sync_inbox(&self, cx: &mut App) {
        let inbox = self.inbox(cx);
        if self.layout == Layout::Focus {
            inbox.update(cx, |i, cx| i.shut(cx));
        } else if inbox.read(cx).open {
            let (cards, observe_only) = (self.inbox_cards(cx), self.agents.observe_only());
            inbox.update(cx, |i, cx| i.set(cards, observe_only, cx));
        }
    }

    fn inbox_cards(&self, cx: &App) -> Vec<Card> {
        let mut cards: Vec<Card> = self
            .agents
            .list
            .iter()
            .filter_map(|a| {
                let status = noted(a)?;
                let asks = self.agents.pending.iter().filter(|p| p.agent_id == a.id).cloned().collect();
                let path = self.project_path(&a.id, cx);
                let project = basename(&path);
                let retry = status == Status::Failed && self.activity_last_prompt(&a.id).is_some();
                Some(Card {
                    agent: a.id.clone(),
                    status,
                    mark: self.project_mark(&path, ui::mark_letter(&project)),
                    project,
                    provider: a.provider.clone(),
                    agent_name: self.agent_name(&a.provider),
                    title: status::title(a),
                    delegated_by: self.agents.get(&a.parent).map(status::title),
                    at: a.updated_at,
                    body: body(status, asks, &a.recap.said, self.chat_shown(&a.terminal_id), &a.error, retry),
                })
            })
            .collect();
        order(&mut cards, self.store.sidebar.inbox_sort);
        cards
    }

    fn project_path(&self, agent: &str, cx: &App) -> String {
        let projects = self.projects(cx);
        let cwd = self.cwd_of(agent, cx).unwrap_or_default();
        self.project_of(&cwd, &projects).cloned().unwrap_or(cwd)
    }

    pub(crate) fn on_inbox_event(&mut self, _: &Entity<Inbox>, ev: &InboxEvent, window: &mut Window, cx: &mut Context<Self>) {
        let pending = |request: &str| self.agents.pending.iter().any(|p| p.request_id == request);
        let answers = !self.agents.observe_only();
        match ev {
            InboxEvent::Open(agent) => {
                if self.agents.get(agent).and_then(noted) != Some(Status::NeedsYou) {
                    self.outbox.seen(std::slice::from_ref(agent));
                }
                self.inbox(cx).update(cx, |i, cx| i.shut(cx));
                self.focus_agent(agent, window, cx);
            }
            InboxEvent::Decide { request, decision } if answers => self.answer_pending(request, *decision, cx),
            InboxEvent::Reply { request, text } if answers && pending(request) => self.outbox.reply(request, text),
            InboxEvent::Answer { request, answers: a } if answers && pending(request) => self.outbox.answer(request, a),
            InboxEvent::Prompt { agent, text } if answers => _ = self.outbox.prompt(agent, text),
            InboxEvent::Retry(agent) if answers => self.retry_turn(agent, cx),
            InboxEvent::Seen(ids) => self.outbox.seen(ids),
            _ => {}
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::{Answers, Body, Card, Status, Tab, body, count, mark_all_seen, opening_tab, order};
    use agents::{Agents, Permission, Summary};
    use store::prefs::sidebar::InboxSort;

    fn agent(id: &str, status: &str, at: i64) -> Summary {
        Summary {
            id: id.into(),
            terminal_id: format!("t-{id}"),
            title: id.to_uppercase(),
            status: status.into(),
            attached: true,
            updated_at: at,
            ..Default::default()
        }
    }

    fn card(agent: &str, status: Status, at: i64) -> Card {
        Card {
            agent: agent.into(),
            status,
            project: String::new(),
            mark: ui::Mark::Letters("A".into()),
            provider: String::new(),
            agent_name: "Claude".into(),
            title: String::new(),
            delegated_by: None,
            at,
            body: Body::Done,
        }
    }

    fn ask(request: &str) -> Permission {
        Permission { request_id: request.into(), agent_id: "a".into(), tool_name: "Bash".into(), ..Default::default() }
    }

    #[test]
    fn the_bell_counts_the_agents_that_need_you_and_the_unseen_failed_and_done() {
        let agents = Agents {
            list: vec![
                agent("done", "done", 3),
                Summary { failed: true, ..agent("fail", "done", 1) },
                agent("ask", "needsYou", 0),
                agent("busy", "working", 9),
                agent("idle", "idle", 9),
                Summary { attached: false, ..agent("blind", "done", 9) },
            ],
            ..Default::default()
        };
        assert_eq!(count(&agents), 3);
    }

    #[test]
    fn a_card_holds_its_open_asks_else_what_it_last_said_its_error_or_nothing_once_done() {
        assert_eq!(body(Status::NeedsYou, vec![ask("r")], "said", true, "", false), Body::Asks(vec![ask("r")]));
        assert_eq!(body(Status::NeedsYou, vec![], " Which branch?\n", false, "", false), Body::Said { text: "Which branch?".into(), chat: false });
        assert_eq!(body(Status::Failed, vec![], "", true, "exit 1\n", true), Body::Failed { error: "exit 1".into(), retry: true });
        assert_eq!(body(Status::Done, vec![], "Tagged v2", true, "", false), Body::Done);
    }

    #[test]
    fn cards_list_needs_you_then_failed_then_done_oldest_first_unless_sorted_by_time() {
        let all = || vec![card("done", Status::Done, 1), card("fail", Status::Failed, 3), card("ask2", Status::NeedsYou, 5), card("ask", Status::NeedsYou, 2)];
        let agents = |sort| {
            let mut cards = all();
            order(&mut cards, sort);
            cards.into_iter().map(|c| c.agent).collect::<Vec<_>>()
        };
        assert_eq!(agents(InboxSort::Urgent), ["ask", "ask2", "fail", "done"]);
        assert_eq!(agents(InboxSort::Newest), ["ask2", "fail", "ask", "done"]);
        assert_eq!(agents(InboxSort::Oldest), ["done", "ask", "fail", "ask2"]);
    }

    #[test]
    fn the_popover_opens_on_needs_you_unless_only_updates_wait() {
        assert_eq!(opening_tab(&[card("a", Status::Done, 0), card("b", Status::Failed, 0)]), Tab::NeedsYou);
        assert_eq!(opening_tab(&[card("a", Status::Done, 0)]), Tab::Updates);
        assert_eq!(opening_tab(&[]), Tab::NeedsYou);
    }

    #[test]
    fn mark_all_seen_marks_only_the_updates() {
        assert_eq!(mark_all_seen(&[card("a", Status::NeedsYou, 0), card("b", Status::Failed, 0), card("c", Status::Done, 0)]), ["c"]);
    }

    #[test]
    fn a_card_steps_through_its_asks_and_settles_once_all_are_answered() {
        let mut answers = Answers::default();
        let c = Card { body: Body::Asks(vec![ask("r1"), ask("r2")]), ..card("a", Status::NeedsYou, 0) };
        let Body::Asks(asks) = &c.body else { unreachable!() };
        assert_eq!(answers.shown(asks).map(|(i, p)| (i, p.request_id.as_str())), Some((0, "r1")));
        assert!(answers.send("r1", "Allowed".into()));
        assert!(!answers.send("r1", "Allowed".into()), "an answer goes out once");
        assert_eq!(answers.shown(asks).map(|(i, _)| i), Some(1));
        assert_eq!(answers.settled(&c), None);
        answers.send("r2", "Denied".into());
        assert_eq!(answers.settled(&c), Some("Denied"));
    }

    #[test]
    fn answers_are_forgotten_once_pocketd_drops_the_ask_or_the_session() {
        let mut answers = Answers::default();
        answers.send("r1", "Allowed".into());
        answers.send("gone", "Sent".into());
        answers.send("fail", "Retrying".into());
        answers.forget_gone(&[Card { body: Body::Asks(vec![ask("r1")]), ..card("a", Status::NeedsYou, 0) }, card("fail", Status::Failed, 0)]);
        let mut kept: Vec<_> = answers.sent.keys().cloned().collect();
        kept.sort();
        assert_eq!(kept, ["fail", "r1"]);
    }
}
