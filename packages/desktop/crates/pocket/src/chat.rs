mod agent_detail;
mod agent_dock;
mod agent_map;
pub(crate) mod composer;
mod diff;
pub(super) mod drafts;
mod end_row;
mod find;
mod find_bar;
mod fold;
mod host_link;
mod jump;
mod locate;
pub(crate) mod markdown;
pub(crate) mod mention;
mod meta;
mod permission;
mod phase;
mod pin;
mod pr_link;
pub(crate) mod question;
mod question_marker;
mod reveal;
pub(crate) mod selectable;
mod selection;
pub(crate) mod shimmer;
mod sticky;
pub(crate) mod sub_agent;
mod thinking;
mod timeline;
mod todos;
mod tool_detail;
pub(crate) mod tool_row;
pub(crate) mod transcript;
pub(crate) mod user_bubble;
pub(crate) mod workflow;
mod working;

use crate::desktop::Desktop;
use crate::explorer::mermaid::Diagrams;
use crate::region::{self, Region};
use agents::Summary;
use gpui_kit::component::input::TextareaState;
use gpui_kit::*;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Instant;
use theme::*;
use workspace::Tab;

/// The key context of a chat, whose ⌘F finds in it.
pub(crate) const CONTEXT: &str = "Chat";

/// Whether an agent's terminal shows its chat: one started from the New session page does, and one without its TUI has nothing else to show.
pub(crate) fn can_chat(started_for_chat: bool, summary: Option<&Summary>) -> bool {
    summary.is_some_and(|a| started_for_chat || a.chat_only)
}

/// How text reaches an agent.
#[derive(Debug, PartialEq)]
pub(crate) enum Route {
    /// Through its chat, which drives an agent whose terminal shows it, with or without a TUI to type into.
    Chat,
    /// Pasted into its TUI.
    Paste,
    /// A TUI without bracketed paste would take each newline as Return.
    Blocked,
}

fn route(chat_shown: bool, bracketed: bool) -> Route {
    match (chat_shown, bracketed) {
        (true, _) => Route::Chat,
        (false, true) => Route::Paste,
        (false, false) => Route::Blocked,
    }
}

pub(crate) struct ChatState {
    pub(crate) input: Entity<TextareaState>,
    pub(crate) views: HashMap<String, Entity<transcript::View>>,
    view_subs: HashMap<String, Subscription>,
    /// Each agent's transcript drawn as its own region, by agent id, with the terminal it draws: its scroll redraws only it.
    transcripts: HashMap<String, (String, Entity<Region>)>,
    pub(crate) composer: Entity<composer::ComposerState>,
    pub(crate) files: Option<locate::Choice>,
    pub(crate) select: Entity<selection::Select>,
    /// The chats whose attachments are being copied to their host; each sends nothing until its copy is done.
    pub(crate) copying: HashSet<String>,
    /// Each chat's attachments copied to its host, by file here, for the send they were copied for.
    pub(crate) copied: HashMap<String, Vec<(PathBuf, String)>>,
    pub(crate) map: Entity<agent_map::AgentMap>,
    pub(crate) dock: Entity<agent_dock::AgentDock>,
    /// Mermaid fences drawn in every chat, sharing one image budget.
    pub(crate) diagrams: Entity<Diagrams>,
    /// The queued entry each agent's composer text was taken from, which its next send replaces.
    pub(crate) editing: HashMap<String, String>,
    pub(crate) find: find_bar::FindBar,
}

impl ChatState {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Desktop>) -> (Self, Vec<Subscription>) {
        let input = composer::input(window, cx);
        let (composer, mut subs) = composer::ComposerState::new(&input, window, cx);
        subs.push(cx.observe(&composer, |_, _, cx| cx.notify()));
        subs.push(cx.subscribe_in(&composer, window, |this, _, ev: &composer::ChatBoxEvent, window, cx| this.on_chat_box_event(ev, window, cx)));
        let map = cx.new(|cx| agent_map::AgentMap::new(cx));
        subs.push(cx.subscribe_in(&map, window, |this, _, ev: &agent_map::MapEvent, window, cx| this.on_map_event(ev, window, cx)));
        subs.push(cx.observe(&map, |_, _, cx| cx.notify()));
        let dock = cx.new(|cx| agent_dock::AgentDock::new(cx));
        subs.push(cx.subscribe_in(&dock, window, |this, _, ev: &agent_dock::DockEvent, window, cx| this.on_dock_event(ev, window, cx)));
        subs.push(cx.observe(&dock, |_, _, cx| cx.notify()));
        let select = cx.new(|_| selection::Select::default());
        subs.push(cx.subscribe_in(&select, window, |this, _, ev: &selection::SelectEvent, window, cx| this.on_select_event(ev, window, cx)));
        subs.push(cx.observe(&select, |_, _, cx| cx.notify()));
        let diagrams = cx.new(|_| Diagrams::standalone(MD_CODE_BG));
        let (find, find_subs) = find_bar::FindBar::new(window, cx);
        subs.extend(find_subs);
        (
            Self {
                input,
                views: HashMap::new(),
                view_subs: HashMap::new(),
                transcripts: HashMap::new(),
                composer,
                files: None,
                select,
                copying: HashSet::new(),
                copied: HashMap::new(),
                map,
                dock,
                diagrams,
                editing: HashMap::new(),
                find,
            },
            subs,
        )
    }
}

impl Desktop {
    /// Whether `terminal`'s pane shows its agent's chat, which it does only while its agent runs.
    pub(crate) fn chat_shown(&self, terminal: &str) -> bool {
        can_chat(self.store.chats.contains(terminal), self.summary(terminal))
    }

    /// How text reaches the agent in `terminal`.
    pub(crate) fn route(&mut self, terminal: &str, cx: &App) -> Route {
        route(self.chat_shown(terminal), self.terminals.read(cx).sessions.term(terminal).is_some_and(|t| t.mode(2004)))
    }

    /// Focuses the terminal pane, and its chat's input when the chat shows: keys then go to the input.
    pub(crate) fn focus_chat_or_terminal(&mut self, terminal: String, window: &mut Window, cx: &mut Context<Self>) {
        let terminal = self.delegated_term(&terminal);
        let chat = self.chat_shown(&terminal).then(|| self.summary(&terminal).map(|a| a.id.clone())).flatten();
        self.focus_pane(terminal, window, cx);
        if let Some(agent) = chat {
            self.chat_focus_input(&agent, window, cx);
        }
    }

    pub(crate) fn chat_focus_input(&mut self, agent: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.chat_take_input(agent, window, cx);
        self.chats.input.update(cx, |s, cx| s.focus(window, cx));
    }

    /// A session's agent can be listed after its terminal took focus; keys then belong to the chat's input.
    pub(crate) fn focus_arrived_chat(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.terminal.focus.is_focused(window) && self.terminal.focused.as_ref().is_some_and(|t| self.chat_shown(t)) {
            self.chats.input.update(cx, |s, cx| s.focus(window, cx));
        }
    }

    /// Hands the input to `agent`'s chat; the chat that had it keeps its draft for when it's back.
    pub(crate) fn chat_take_input(&mut self, agent: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.chats.composer.update(cx, |c, cx| c.take_input(agent, window, cx));
    }

    /// Drops a closed agent's chat, with its draft and what it never sent.
    pub(crate) fn chat_closed(&mut self, agent: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.chats.composer.update(cx, |c, cx| c.close_draft(agent, window, cx));
        self.chats.transcripts.remove(agent);
        self.chats.view_subs.remove(agent);
        self.delegated_forget(agent);
        self.chats.views.remove(agent);
        self.chat_prune_dock(cx);
    }

    /// An agent terminal's body while it shows the chat: the transcript over the composer.
    pub(crate) fn chat_view(&mut self, terminal: &str, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let agent = self.summary(terminal).map(|s| s.id.clone());
        if let Some(agent) = agent.as_ref().filter(|_| self.terminal.focused.as_deref() == Some(terminal)) {
            self.chat_take_input(agent, window, cx);
        }
        let transcript = match &agent {
            Some(agent) => {
                if self.chats.transcripts.get(agent).is_none_or(|(t, _)| t != terminal) {
                    let t = terminal.to_string();
                    let region = Region::new(cx, move |d, window, cx| match d.chat_transcript(&t, window, cx) {
                        Some(view) => view.into_any_element(),
                        None => div().flex_1().into_any_element(),
                    });
                    self.chats.transcripts.insert(agent.clone(), (terminal.to_string(), region));
                }
                let region = region::cached(&self.chats.transcripts[agent].1, StyleRefinement::default().flex_1().min_h_0());
                self.offers_selection(agent, region, cx).into_any_element()
            }
            None => div().flex_1().into_any_element(),
        };
        let composer = self.chat_composer(terminal, window, cx);
        let composer = match &agent {
            Some(agent) => crate::creating::arriving(&self.creates, agent, crate::creating::DOCK, composer, window, cx),
            None => composer,
        };
        let crumb = agent.as_ref().and_then(|agent| self.delegated_crumb(agent, window, cx));
        let panel = agent.as_ref().and_then(|agent| self.delegated_panel(agent, window, cx));
        let pushed = agent.as_ref().and_then(|agent| self.delegated_pushed(agent, window, cx));
        let dock = agent.as_ref().and_then(|agent| self.chat_dock(agent, cx));
        let find = agent.as_ref().and_then(|agent| self.chat_find_bar(agent, cx));
        let files = agent.and_then(|agent| self.chat_files_menu(&agent, cx));
        let drop = self.chat_drop_zone(terminal, cx);
        let shown = div().relative().size_full().flex().flex_col().child(transcript).children(crumb).children(panel);
        let (shown, left) = match pushed {
            Some((from, frame)) => (shown.left(px(frame.arriving.0)).opacity(frame.arriving.1), self.chat_left(&from, frame.leaving, window, cx)),
            None => (shown, None),
        };
        let stage = div().relative().flex_1().min_h_0().children(left).child(shown).children(find);
        let backdrop = self.store.appearance.background.clone().map(|path| crate::field::backdrop(path, crate::field::CHAT_TINT));
        let column =
            div().relative().flex_1().min_w_0().min_h_0().flex().flex_col().children(backdrop).child(stage).child(composer).children(files).children(drop);
        div().key_context(CONTEXT).relative().flex_1().min_h_0().flex().border_t(px(0.5)).border_color(SEPARATOR).child(column).children(dock)
    }

    fn chat_transcript(&mut self, terminal: &str, window: &mut Window, cx: &mut Context<Self>) -> Option<Entity<transcript::View>> {
        let agent = self.summary(terminal)?.clone();
        let id = agent.id.clone();
        let view = match self.chats.views.get(&id) {
            Some(view) => view.clone(),
            None => {
                let view = cx.new(|cx| transcript::View::new(&id, self.creates.clone(), self.chats.diagrams.clone(), cx));
                // A chat opened from its create shows the prompt pocketd sent before the transcript holds it.
                if let Some(prompt) = self.creates.read(cx).unsent(&id, terminal, Instant::now()).map(str::to_string) {
                    view.update(cx, |v, _| v.echo(prompt));
                }
                let key = id.clone();
                let sub = cx.subscribe_in(&view, window, move |this, _, ev: &transcript::ViewEvent, window, cx| this.on_chat_view(&key, ev, window, cx));
                self.chats.view_subs.insert(id.clone(), sub);
                self.chats.views.insert(id.clone(), view.clone());
                view
            }
        };
        let released = view.update(cx, |v, _| {
            v.refresh(&agent, self.agents.revision(&id), self.agents.timelines.get(&id).map_or(&[][..], Vec::as_slice), self.agents.delegations.of(&id))
        });
        self.release_sends(&id, &view, released, cx);
        let side = self.transcript_side(&agent, &view, cx);
        view.update(cx, |v, _| v.set_side(side));
        Some(view)
    }

    fn release_sends(&mut self, id: &str, view: &Entity<transcript::View>, released: Vec<(u64, String)>, cx: &mut Context<Self>) {
        for (send, text) in released {
            if let Some(request) = self.outbox.prompt(id, &text) {
                view.update(cx, |v, _| v.sent_as(send, request));
            }
        }
    }

    fn transcript_side(&self, agent: &Summary, view: &Entity<transcript::View>, cx: &App) -> transcript::Side {
        let id = &agent.id;
        let ask = self.agents.pending.iter().find(|p| p.agent_id == *id).map(|p| !chat::for_provider(&agent.provider).ask(p).questions.is_empty());
        let (links_pr, has_host_link) = {
            let v = view.read(cx);
            (v.links_pr(), v.has_host_link())
        };
        transcript::Side {
            agent: agent.clone(),
            inset: self.delegated_inset(id),
            ask,
            send_now: self.chat_caps(&agent.provider).send_now,
            delegated_by: self.delegated_by(id),
            here: self.store.home().join("attachments"),
            pick_open: self.chats.dock.read(cx).pick.open_in(id).map(str::to_string),
            delegations: self.agents.delegations.of(id).to_vec(),
            hosts: has_host_link.then(|| (self.fleet.hosts.list.clone(), self.fleet.hosts.links.clone())),
            capturing: self.capturing,
            // `fix_agent` walks every agent, so only a chat that links a PR asks.
            prs: links_pr.then(|| {
                let to = pr_link::FixTo { tree: self.cwd(cx), agent: self.fix_agent(cx), can_send: !self.agents.observe_only() };
                pr_link::peeks(&self.prs, self.pr.read(cx), &to)
            }),
        }
    }

    fn on_chat_view(&mut self, agent: &str, ev: &transcript::ViewEvent, window: &mut Window, cx: &mut Context<Self>) {
        use transcript::ViewEvent::*;
        match ev {
            Open(link, press) => {
                let cwd = self.agents.get(agent).map(|a| a.cwd.clone()).unwrap_or_default();
                self.chat_link(agent, link, &cwd, *press, window, cx);
            }
            Command(command) => self.chat_command(agent, command.clone(), cx),
            TakeBack(text) => self.chat_take_back(agent, text, window, cx),
            Pick(call) => self.chat_pick_agent(agent, call, window, cx),
            OpenSession(id) => self.delegated_open(id, window, cx),
            LinkHost(hint) => self.link_host(hint.clone(), window, cx),
            ShowHosts => self.show_hosts(window, cx),
            ShowPr => self.show_pr_side(cx),
            SendFix(checks, threads) => self.send_fix(checks, threads, cx),
            Scrolled(away) => self.delegated_scrolled(agent, *away, cx),
            ReachedTop => self.outbox.older(agent),
        }
    }

    /// Refolds every chat as its timeline moves, on screen or not, and sends each one's next queued send once its agent goes
    /// idle. A chat away from the screen streams its text in as it arrives, so it isn't let out again on return.
    pub(crate) fn refresh_chats(&mut self, cx: &mut Context<Self>) {
        let views: Vec<(String, Entity<transcript::View>)> = self.chats.views.iter().map(|(id, v)| (id.clone(), v.clone())).collect();
        for (id, view) in &views {
            let Some(agent) = self.agents.get(id) else { continue };
            let items = self.agents.timelines.get(id).map_or(&[][..], Vec::as_slice);
            let released = view.update(cx, |v, _| v.refresh(agent, self.agents.revision(id), items, self.agents.delegations.of(id)));
            self.release_sends(id, view, released, cx);
        }
        for id in self.chats.views.keys().filter(|id| self.agents.holds_queue(id)).cloned().collect::<Vec<_>>() {
            if self.agents.queues.ask(&id) {
                self.outbox.queue_list(&id);
            }
        }
        self.chat_prune_dock(cx);
    }

    /// Scrolls `agent`'s chat to the row showing block `block` of turn `turn`, opened, and flashes it.
    pub(crate) fn chat_show_block(&mut self, agent: &str, turn: usize, block: usize, cx: &mut Context<Self>) {
        let Some(view) = self.chats.views.get(agent) else { return };
        view.update(cx, |v, cx| v.show_block(turn, block, cx));
        cx.notify();
    }

    /// Scrolls the shown chat a prompt up or down from the exchange at its top.
    pub(crate) fn chat_step_prompt(&mut self, down: bool, cx: &mut Context<Self>) {
        let Some(view) = self.session().and_then(|id| self.chats.views.get(id)) else { return };
        view.update(cx, |v, cx| v.step_prompt(down, cx));
    }

    /// Opens a page in the browser the settings pick, or a file in the preview.
    pub(super) fn chat_open(&mut self, agent: &str, link: &str, cwd: &str, window: &mut Window, cx: &mut Context<Self>) {
        match markdown::target(link, cwd, &self.host_agents.home_of(cwd)) {
            markdown::Target::Web(url) if self.store.terminal.system_browser => cx.open_url(&url),
            markdown::Target::Web(url) => self.open_browser(Some(url), window, cx),
            markdown::Target::Session(id) => self.delegated_open(&id, window, cx),
            markdown::Target::File(path) => self.chat_open_file(agent, path, cwd, window.mouse_position(), cx),
        }
    }

    /// The chat `from` as it was, sliding and fading out under the one arriving: the transcript as last drawn, with its breadcrumb and panel. None where it also shows in a pane, which draws it once.
    fn chat_left(&mut self, from: &str, (across, shown): (f32, f32), window: &mut Window, cx: &mut Context<Self>) -> Option<Div> {
        let terminal = self.agents.get(from)?.terminal_id.clone();
        let on_screen = self
            .session_tree(cx)
            .and_then(|t| self.workspaces.read(cx).get(&t))
            .is_some_and(|w| w.shown().iter().any(|(_, tab)| matches!(tab, Tab::Term(id) if self.delegated_term(id) == terminal)));
        if on_screen {
            return None;
        }
        let transcript = region::cached(&self.chats.transcripts.get(from)?.1, StyleRefinement::default().flex_1().min_h_0());
        let crumb = self.delegated_crumb(from, window, cx);
        let panel = self.delegated_panel(from, window, cx);
        Some(
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .left(px(across))
                .right(px(-across))
                .flex()
                .flex_col()
                .opacity(shown)
                .child(transcript)
                .children(crumb)
                .children(panel),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{Route, Summary, can_chat, route};

    #[test]
    fn text_goes_through_the_chat_of_an_agent_its_chat_drives_even_without_bracketed_paste() {
        assert_eq!(route(true, false), Route::Chat);
        assert_eq!(route(true, true), Route::Chat);
        assert_eq!(route(false, true), Route::Paste);
        assert_eq!(route(false, false), Route::Blocked, "pasted unbracketed, the text's first newline would send it");
    }

    #[test]
    fn chat_shows_only_for_terminals_that_run_an_agent() {
        let agent = Summary { id: "a".into(), terminal_id: "t".into(), ..Default::default() };
        assert!(can_chat(true, Some(&agent)));
        assert!(!can_chat(true, None));
    }

    #[test]
    fn an_agent_only_the_chat_drives_shows_its_chat_wherever_it_was_started() {
        let tui = Summary { id: "a".into(), terminal_id: "t".into(), ..Default::default() };
        assert!(!can_chat(false, Some(&tui)));
        assert!(can_chat(false, Some(&Summary { chat_only: true, ..tui })));
    }
}
