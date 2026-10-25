use crate::chat::locate::touched;
use crate::composer::model_menu::{PAD, ROW, row, rows};
use crate::desktop::Desktop;
use crate::palette::{marked, matches};
use agents::{McpAction, McpServer, McpStatus};
use chat::{Kind, Local, Slash};
use gpui_kit::component::input::{Enter, Escape, IndentInline, MoveDown, MoveUp, RopeExt as _, TextareaState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::time::Duration;
use theme::*;

/// The rows the list shows at once.
const SHOWN: usize = 8;

/// How often the servers are asked again while one settles: a sign-in waits on the browser, which tells us nothing
/// when it ends, or a server still connects.
const SETTLE_CHECK: Duration = Duration::from_secs(3);

/// The `/` and `@` list over the composer, and what it lists from.
#[derive(Default)]
pub(crate) struct Suggest {
    /// Each agent's commands as its CLI lists them, by agent id; empty until they arrive.
    commands: HashMap<String, Vec<agents::Command>>,
    /// Each folder's files as git lists them, relative to it; empty while they load.
    files: HashMap<String, Vec<String>>,
    /// Each agent's MCP servers as it last told them, or why it couldn't, by agent id; `None` while the first answer
    /// is on its way.
    servers: HashMap<String, Option<Result<Vec<McpServer>, String>>>,
    /// The server each agent is acting on, by agent id, until its answer.
    acting: HashMap<String, (String, McpAction)>,
    /// The servers whose sign-in went to the browser, as (agent id, server), until they no longer wait on one.
    signing_in: HashSet<(String, String)>,
    /// The agents whose servers are to be asked again as they settle, until they are.
    rechecking: HashSet<String>,
    list: Option<List>,
    selected: usize,
}

/// What the list shows, worked out as the draft changes rather than each frame.
struct List {
    agent: String,
    /// The draft it was worked out for; a draft set without a change, as a send's clearing, hides it.
    text: String,
    rows: Rows,
}

enum Rows {
    /// `at` is where the `/` stands.
    Commands { at: usize, query: String, rows: Vec<Slash> },
    /// `at` is where the `@` stands.
    Files { at: usize, query: String, paths: Vec<String> },
    /// `at` is where the `@mcp/` tag stands; `servers` are those that hold the query, as `chat::mcp::picker` orders
    /// them, and `None` while they load. The first `pickable` take a tag or an action.
    Servers { at: usize, query: String, servers: Option<Result<Vec<McpServer>, String>>, pickable: usize },
}

impl Rows {
    /// The rows a pick can take.
    fn len(&self) -> usize {
        match self {
            Rows::Commands { rows, .. } => rows.len(),
            Rows::Files { paths, .. } => paths.len(),
            Rows::Servers { pickable, .. } => *pickable,
        }
    }
}

fn listed(servers: &Option<Result<Vec<McpServer>, String>>) -> &[McpServer] {
    match servers {
        Some(Ok(servers)) => servers,
        _ => &[],
    }
}

/// What follows the `@` of a server's tag.
fn mcp_word() -> &'static str {
    &chat::mcp::TAG[1..]
}

/// Whether `provider`'s chat takes `/mcp`.
fn offers_mcp(provider: &str) -> bool {
    chat::for_provider(provider).command("mcp", "") == Kind::Local(Local::Mcp)
}

impl Suggest {
    pub(crate) fn listed(&mut self, agent: String, commands: Vec<agents::Command>) {
        self.commands.insert(agent, commands);
    }

    /// Takes `agent`'s servers; once `acted` on one, a sign-in URL means it now waits on the browser.
    pub(crate) fn mcp_listed(&mut self, agent: String, servers: Result<Vec<McpServer>, String>, auth_url: &str, acted: bool) {
        if acted
            && let Some((server, McpAction::Auth)) = self.acting.remove(&agent)
            && !auth_url.is_empty()
        {
            self.signing_in.insert((agent.clone(), server));
        }
        if let Ok(list) = &servers {
            let waits = |name: &str| list.iter().any(|s| s.name == name && s.status == McpStatus::NeedsAuth);
            self.signing_in.retain(|(a, name)| *a != agent || waits(name));
        }
        self.servers.insert(agent, Some(servers));
    }

    pub(crate) fn mcp_act_failed(&mut self, agent: &str) {
        self.acting.remove(agent);
    }

    /// What `agent` is to do to `s` now, as `provider` can; nothing while it acts on another, one at a time.
    fn act(&mut self, agent: &str, provider: &str, s: &McpServer) -> Option<McpAction> {
        let action = chat::mcp::action(provider, s).filter(|_| !self.acting.contains_key(agent))?;
        self.acting.insert(agent.into(), (s.name.clone(), action));
        Some(action)
    }

    fn signs_in(&self, agent: &str, server: &str) -> bool {
        self.signing_in.contains(&(agent.to_string(), server.to_string()))
    }

    /// Whether `agent` waits on a browser sign-in or a server that still connects.
    fn settling(&self, agent: &str) -> bool {
        let connecting = self.servers.get(agent).is_some_and(|l| listed(l).iter().any(|s| s.status == McpStatus::Pending));
        connecting || self.signing_in.iter().any(|(a, _)| a == agent)
    }

    /// Whether to ask `agent`'s servers again shortly: they settle and no ask is due yet.
    fn rechecks(&mut self, agent: &str) -> bool {
        self.settling(agent) && self.rechecking.insert(agent.into())
    }

    /// `text` led by a line asking `agent` to use the MCP servers it tags.
    pub(crate) fn with_mcp_context(&self, agent: &str, provider: &str, text: &str) -> String {
        match self.servers.get(agent) {
            Some(Some(Ok(servers))) => chat::mcp::with_context(text, servers, provider),
            _ => text.to_string(),
        }
    }

    /// Whether to ask `agent` for its MCP servers: each time a tag starts, as they connect and drop, and while none
    /// came yet.
    fn asks_servers(&mut self, agent: &str, text: &str, cursor: usize) -> bool {
        let tag = word(text, cursor, '@').and_then(|(_, q)| q.strip_prefix(mcp_word()));
        let ask = tag.is_some_and(|q| q.is_empty() || !self.servers.contains_key(agent));
        if ask {
            self.servers.entry(agent.into()).or_insert(None);
        }
        ask
    }

    /// Whether to ask `agent` for its commands: each time a `/` starts a word, as they can change, and while none
    /// came yet.
    fn asks_commands(&mut self, agent: &str, text: &str, cursor: usize) -> bool {
        let ask = word(text, cursor, '/').is_some_and(|(_, q)| q.is_empty() || !self.commands.contains_key(agent));
        if ask {
            self.commands.entry(agent.into()).or_default();
        }
        ask
    }

    /// Whether to list `cwd`'s files: each time an `@` starts a mention, and while none came yet.
    fn loads_files(&mut self, cwd: &str, text: &str, cursor: usize) -> bool {
        let load = word(text, cursor, '@').is_some_and(|(_, q)| q.is_empty() || !self.files.contains_key(cwd));
        if load {
            self.files.entry(cwd.into()).or_default();
        }
        load
    }

    /// Moves the selection one row, wrapping at either end of `total`.
    fn step(&mut self, down: bool, total: usize) {
        let at = self.selected.min(total - 1);
        self.selected = if down { (at + 1) % total } else { (at + total - 1) % total };
    }
}

/// Whether the `/` at `at` starts the draft as `chat::slash` takes it, after whitespace at most.
fn starts_draft(text: &str, at: usize) -> bool {
    text[..at].trim().is_empty()
}

/// The draft with command `name` and `arg`, and where the cursor goes: after the name, ready for the argument.
fn named(name: &str, arg: &str) -> (String, usize) {
    (format!("/{name} {arg}"), name.len() + 2)
}

/// Where the `sigil` word the cursor ends starts, and what follows the `sigil`; it starts the draft or follows
/// whitespace.
fn word(text: &str, cursor: usize, sigil: char) -> Option<(usize, &str)> {
    let before = text.get(..cursor)?;
    let start = before.char_indices().rev().find(|(_, c)| c.is_whitespace()).map_or(0, |(i, c)| i + c.len_utf8());
    before[start..].strip_prefix(sigil).map(|query| (start, query))
}

/// The `files` that hold the query: those the agent touched, latest first, then those changed, then
/// the rest in git's order.
fn rank(query: &str, touched: &[String], changed: &HashSet<&str>, files: &[String]) -> Vec<String> {
    let words: Vec<String> = Some(query.to_lowercase()).filter(|q| !q.is_empty()).into_iter().collect();
    let touched: HashMap<&str, usize> = touched.iter().enumerate().rev().map(|(i, t)| (t.as_str(), i)).collect();
    let tier = |&(i, f): &(usize, &String)| match touched.get(f.as_str()) {
        Some(&t) => (0, t),
        None if changed.contains(f.as_str()) => (1, i),
        None => (2, i),
    };
    let mut found: Vec<(usize, &String)> = files.iter().enumerate().filter(|(_, f)| matches(&words, &[f.as_str()])).collect();
    found.sort_unstable_by_key(tier);
    found.into_iter().map(|(_, f)| f.clone()).collect()
}

/// The `SHOWN` rows around `selected` of `total`, scrolled only as far as it takes to keep it in view.
fn window(selected: usize, total: usize) -> Range<usize> {
    let start = selected.saturating_sub(SHOWN - 1).min(total.saturating_sub(SHOWN));
    start..total.min(start + SHOWN)
}

/// The draft with `put` in place of the word from `at` to `cursor`, and where the cursor goes after it.
fn replaced(text: &str, at: usize, cursor: usize, put: &str) -> (String, usize) {
    let (head, rest) = (format!("{}{put}", &text[..at]), &text[cursor..]);
    let gap = if rest.starts_with(char::is_whitespace) { "" } else { " " };
    (format!("{head}{gap}{rest}"), head.len() + 1)
}

/// A server's row: which agent has it and where, and how it stands, or what picking it does when the agent can't use it
/// now; one it can do nothing about is dimmed.
fn server_row(r: Stateful<Div>, s: &McpServer, provider: &str, query: &str, signing_in: bool, acting: Option<McpAction>) -> Stateful<Div> {
    let status = match s.status {
        McpStatus::Connected => "Available",
        McpStatus::Pending => "Connecting",
        McpStatus::NeedsAuth if signing_in => "Finish signing in in your browser",
        McpStatus::NeedsAuth => "Needs authentication",
        McpStatus::Failed => "Failed to connect",
        McpStatus::Disabled => "Disabled",
    };
    let action = chat::mcp::action(provider, s);
    let (label, color) = match (acting, action) {
        (Some(McpAction::Auth), _) => ("Signing in…", TEXT_3),
        (Some(McpAction::Reconnect), _) => ("Reconnecting…", TEXT_3),
        (Some(McpAction::Enable), _) => ("Enabling…", TEXT_3),
        (None, Some(McpAction::Auth)) if signing_in => ("Sign in again", WAITING_TEXT),
        (None, Some(McpAction::Auth)) => ("Sign in", WAITING_TEXT),
        (None, Some(McpAction::Reconnect)) => ("Reconnect", ACCENT_LINK),
        (None, Some(McpAction::Enable)) => ("Enable", ACCENT_LINK),
        (None, None) if chat::mcp::usable(s) => (status, TEXT_3),
        (None, None) => (status, TEXT_4),
    };
    let mut place = vec![provider_name(provider), s.scope.as_str()];
    if action.is_some() {
        place.push(status);
    }
    let place = place.into_iter().filter(|p| !p.is_empty()).collect::<Vec<_>>().join(" · ");
    let name = div().truncate().child(marked(s.name.clone(), &[query.to_lowercase()]));
    r.h(px(44.))
        .when(!chat::mcp::usable(s) && action.is_none(), |r| r.cursor_default().hover(|s| s).text_color(TEXT_4))
        .child(provider_icon(provider, 14., TEXT_2))
        .child(div().flex_1().min_w_0().flex().flex_col().child(name).child(div().truncate().text_size(px(11.)).text_color(TEXT_3).child(place)))
        .child(div().flex_none().text_size(px(11.)).text_color(color).child(label))
}

/// A row saying why the list has no server to pick.
fn note(text: &str, color: Token) -> Stateful<Div> {
    div().id("chat-suggest-note").min_h(px(ROW)).px(px(10.)).py(px(6.)).flex().items_center().text_size(px(12.)).text_color(color).child(text.to_string())
}

/// The composer a list serves.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Prompt {
    Chat,
    /// The new session's, whose agent doesn't run yet.
    Launch,
}

/// Who a draft goes to: the chat's agent, or the agent, account and project a new session starts; `key` is the agent's
/// id or `launch_key`'s.
struct Target {
    key: String,
    provider: String,
    account: String,
    cwd: String,
}

/// What a new session's commands go by: its provider, account and project.
pub(crate) fn launch_key(provider: &str, account: &str, cwd: &str) -> String {
    format!("{provider}\0{account}\0{cwd}")
}

impl Desktop {
    fn suggest_input(&self, p: Prompt) -> &Entity<TextareaState> {
        match p {
            Prompt::Chat => &self.chats.input,
            Prompt::Launch => &self.new_form.composer.prompt,
        }
    }

    fn suggest_state(&self, p: Prompt) -> &Suggest {
        match p {
            Prompt::Chat => &self.chats.composer.suggest,
            Prompt::Launch => &self.new_form.composer.suggest,
        }
    }

    fn suggest_state_mut(&mut self, p: Prompt) -> &mut Suggest {
        match p {
            Prompt::Chat => &mut self.chats.composer.suggest,
            Prompt::Launch => &mut self.new_form.composer.suggest,
        }
    }

    fn suggest_target(&self, p: Prompt) -> Option<Target> {
        match p {
            Prompt::Chat => self.chats.drafts.owner().and_then(|id| self.agents.get(id)).map(|a| Target { key: a.id.clone(), provider: a.provider.clone(), account: a.account.clone(), cwd: a.cwd.clone() }),
            Prompt::Launch => {
                let pick = self.new_form_pick();
                self.new_form.project().map(|cwd| Target { key: launch_key(&pick.provider, &pick.account, cwd), provider: pick.provider, account: pick.account, cwd: cwd.into() })
            }
        }
    }

    /// Asks for what the list needs as the draft changes: the agent's commands each time a `/` starts one, or a new
    /// session's skills as pocketd reads them off disk, its MCP servers each time an `@mcp/` tag does, and the folder's
    /// files each time another `@` does.
    pub(crate) fn draft_changed(&mut self, p: Prompt, cx: &mut Context<Self>) {
        self.suggest_state_mut(p).selected = 0;
        let Some(t) = self.suggest_target(p) else { return self.suggest(p, cx) };
        let input = self.suggest_input(p).read(cx);
        let (text, cursor) = (input.value().to_string(), input.cursor());
        let s = self.suggest_state_mut(p);
        if s.asks_commands(&t.key, &text, cursor) {
            match p {
                Prompt::Chat => self.outbox.commands(&t.key),
                Prompt::Launch => self.outbox.skills(&t.provider, &t.account, &t.cwd),
            }
        }
        if p == Prompt::Chat && offers_mcp(&t.provider) && word(&text, cursor, '@').is_some_and(|(_, q)| q.starts_with(mcp_word())) {
            if self.suggest_state_mut(p).asks_servers(&t.key, &text, cursor) {
                self.outbox.mcp(&t.key);
            }
        } else if self.suggest_state_mut(p).loads_files(&t.cwd, &text, cursor) {
            self.load_files(p, &t.cwd, cx);
        }
        self.suggest(p, cx);
    }

    /// Works the list out again for the draft as it stands. A new session lists only the commands sent as prompts, as
    /// nothing runs yet; a `/` past the draft's start lists those and `/mcp`, as another command runs only from the
    /// start.
    pub(crate) fn suggest(&mut self, p: Prompt, cx: &mut Context<Self>) {
        let t = self.suggest_target(p);
        let input = self.suggest_input(p).read(cx);
        let (text, cursor) = (input.value().to_string(), input.cursor());
        let s = self.suggest_state(p);
        let rows = t.as_ref().and_then(|t| {
            if let Some((at, query)) = word(&text, cursor, '/') {
                let listed = s.commands.get(&t.key).map_or(&[][..], Vec::as_slice);
                let mut rows = chat::menu(chat::for_provider(&t.provider), listed, query);
                if p == Prompt::Launch {
                    rows.retain(|r| r.kind == Kind::Send);
                } else if !starts_draft(&text, at) {
                    rows.retain(|r| matches!(r.kind, Kind::Send | Kind::Local(Local::Mcp)));
                }
                return Some(Rows::Commands { at, query: query.into(), rows });
            }
            let (at, query) = word(&text, cursor, '@')?;
            if let Some(query) = query.strip_prefix(mcp_word()).filter(|_| p == Prompt::Chat && offers_mcp(&t.provider)) {
                let servers = s.servers.get(&t.key).cloned().flatten().map(|r| r.map(|all| chat::mcp::picker(&all, query)));
                let pickable = listed(&servers).iter().take_while(|x| chat::mcp::usable(x) || chat::mcp::action(&t.provider, x).is_some()).count();
                return Some(Rows::Servers { at, query: query.into(), servers, pickable });
            }
            let root = format!("{}/", hosts::split(&t.cwd).1.trim_end_matches('/'));
            let views = (p == Prompt::Chat).then(|| self.chats.views.get(&t.key)).flatten();
            let touched: Vec<String> = views.map(|v| touched(&v.session.transcript)).unwrap_or_default().iter().filter_map(|p| p.strip_prefix(&root).map(String::from)).collect();
            let changed: HashSet<&str> = self.repos.get(&t.cwd).map(|r| r.files.iter().map(|f| f.path.as_str()).collect()).unwrap_or_default();
            let files = s.files.get(&t.cwd).map_or(&[][..], Vec::as_slice);
            Some(Rows::Files { at, query: query.into(), paths: rank(query, &touched, &changed, files) })
        });
        let list = t.zip(rows).map(|(t, rows)| List { agent: t.key, text, rows });
        self.suggest_state_mut(p).list = list;
        cx.notify();
    }

    /// Sets the draft as if typed, so the list follows it.
    pub(crate) fn type_text(&mut self, p: Prompt, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.put_text(p, text.into(), None, window, cx);
        self.draft_changed(p, cx);
    }

    fn load_files(&mut self, p: Prompt, cwd: &str, cx: &mut Context<Self>) {
        let (cwd, runner) = (cwd.to_string(), hosts::runner_for(cwd));
        let task = cx.background_executor().spawn({
            let cwd = cwd.clone();
            async move { git::ls_files(&runner, &cwd) }
        });
        cx.spawn(async move |this, cx| {
            let files = task.await;
            this.update(cx, |d, cx| {
                d.suggest_state_mut(p).files.insert(cwd, files);
                d.suggest(p, cx);
            })
            .ok();
        })
        .detach();
    }

    /// The list, while the input holds the draft it was worked out for and it has rows; servers show while they load
    /// or when none fit, to say so.
    fn suggestions(&self, p: Prompt, cx: &App) -> Option<&List> {
        let list = self.suggest_state(p).list.as_ref()?;
        let current = self.suggest_target(p).is_some_and(|t| t.key == list.agent) && *self.suggest_input(p).read(cx).value() == *list.text;
        (current && (list.rows.len() > 0 || matches!(list.rows, Rows::Servers { .. }))).then_some(list)
    }

    fn suggest_move(&mut self, p: Prompt, down: bool, cx: &mut Context<Self>) -> bool {
        let Some(total) = self.suggestions(p, cx).map(|l| l.rows.len()) else { return false };
        if total > 0 {
            self.suggest_state_mut(p).step(down, total);
            cx.notify();
        }
        true
    }

    /// Puts the picked row in the draft: a command's name, ready for its argument, a file's path, or a server's tag.
    /// A command the desktop runs itself runs at once; `/mcp` gives way to a tag, which lists the servers. A server the
    /// agent can't use now can't be picked.
    fn suggest_pick(&mut self, p: Prompt, ix: usize, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(shown) = self.suggestions(p, cx) else { return false };
        if let Rows::Servers { servers, pickable, .. } = &shown.rows {
            if ix >= *pickable {
                return false;
            }
            let s = &listed(servers)[ix];
            if !chat::mcp::usable(s) {
                let (agent, s) = (shown.agent.clone(), s.clone());
                self.mcp_act(p, &agent, &s, cx);
                return true;
            }
        }
        let Some(list) = self.suggest_state_mut(p).list.take() else { return false };
        match list.rows {
            Rows::Commands { at, rows, .. } if !starts_draft(&list.text, at) => {
                let Some(c) = rows.get(ix) else { return true };
                let cursor = self.suggest_input(p).read(cx).cursor();
                if c.kind == Kind::Local(Local::Mcp) {
                    let text = format!("{}{}{}", &list.text[..at], chat::mcp::TAG, &list.text[cursor..]);
                    self.put_text(p, text, Some(at + chat::mcp::TAG.len()), window, cx);
                    self.draft_changed(p, cx);
                    return true;
                }
                let (text, cursor) = replaced(&list.text, at, cursor, &format!("/{}", c.name));
                self.put_text(p, text, Some(cursor), window, cx);
            }
            Rows::Commands { rows, .. } => {
                let Some(c) = rows.get(ix) else { return true };
                let arg = chat::slash(&list.text).map_or("", |(_, arg)| arg);
                let provider = self.suggest_target(p).map(|t| t.provider).unwrap_or_default();
                // The list sorted it without an argument, which can change it: Claude sends `/compact` with one.
                match chat::for_provider(&provider).command(&c.name, arg) {
                    Kind::Local(local) if p == Prompt::Chat => {
                        self.set_chat_text("", window, cx);
                        self.chat_local(&list.agent, local, arg, window, cx);
                    }
                    _ => {
                        let (text, cursor) = named(&c.name, arg);
                        self.put_text(p, text, Some(cursor), window, cx);
                    }
                }
            }
            Rows::Files { at, paths, .. } => {
                let Some(path) = paths.get(ix) else { return true };
                let cursor = self.suggest_input(p).read(cx).cursor();
                let (text, cursor) = replaced(&list.text, at, cursor, &format!("@{path}"));
                self.put_text(p, text, Some(cursor), window, cx);
            }
            Rows::Servers { at, servers, .. } => {
                let cursor = self.suggest_input(p).read(cx).cursor();
                let (text, cursor) = replaced(&list.text, at, cursor, &format!("{}{}", chat::mcp::TAG, listed(&servers)[ix].name));
                self.put_text(p, text, Some(cursor), window, cx);
            }
        }
        true
    }

    /// Has `agent` sign in to, reconnect or enable `s`, the list staying open to show how it went; one at a time.
    fn mcp_act(&mut self, p: Prompt, agent: &str, s: &McpServer, cx: &mut Context<Self>) {
        let Some(t) = self.suggest_target(p) else { return };
        let Some(action) = self.suggest_state_mut(p).act(agent, &t.provider, s) else { return };
        self.outbox.mcp_act(agent, action, &s.name);
        cx.notify();
    }

    /// Asks `agent`'s servers again shortly while they settle and the chat lists them.
    pub(crate) fn recheck_servers(&mut self, agent: String, cx: &mut Context<Self>) {
        if !self.chats.composer.suggest.rechecks(&agent) {
            return;
        }
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SETTLE_CHECK).await;
            _ = this.update(cx, |d, cx| {
                d.chats.composer.suggest.rechecking.remove(&agent);
                let listing = d.suggestions(Prompt::Chat, cx).is_some_and(|l| l.agent == agent && matches!(l.rows, Rows::Servers { .. }));
                if listing && d.chats.composer.suggest.settling(&agent) {
                    d.outbox.mcp(&agent);
                }
            });
        })
        .detach();
    }

    fn put_text(&mut self, p: Prompt, text: String, cursor: Option<usize>, window: &mut Window, cx: &mut Context<Self>) {
        let cursor = cursor.unwrap_or(text.len());
        self.suggest_input(p).clone().update(cx, |s, cx| {
            s.set_value(text, window, cx);
            let at = s.text().offset_to_position(cursor);
            s.set_cursor_position(at, window, cx);
        });
        cx.notify();
    }

    /// Takes the list's keys while it shows, before the input's own bindings do.
    pub(crate) fn suggest_keys(&self, p: Prompt, d: Div, cx: &mut Context<Self>) -> Div {
        let stop = |cx: &mut Context<Self>, taken: bool| {
            if taken {
                cx.stop_propagation();
            }
        };
        d.capture_action(cx.listener(move |this, _: &MoveUp, _, cx| {
            let taken = this.suggest_move(p, false, cx);
            stop(cx, taken);
        }))
        .capture_action(cx.listener(move |this, _: &MoveDown, _, cx| {
            let taken = this.suggest_move(p, true, cx);
            stop(cx, taken);
        }))
        .capture_action(cx.listener(move |this, _: &IndentInline, window, cx| {
            let taken = this.suggest_pick(p, this.suggest_state(p).selected, window, cx);
            stop(cx, taken);
        }))
        .capture_action(cx.listener(move |this, e: &Enter, window, cx| {
            let taken = !e.secondary && !e.shift && this.suggest_pick(p, this.suggest_state(p).selected, window, cx);
            stop(cx, taken);
        }))
        .capture_action(cx.listener(move |this, _: &Escape, _, cx| {
            let taken = this.suggestions(p, cx).is_some();
            if taken {
                this.suggest_state_mut(p).list = None;
                cx.notify();
            }
            stop(cx, taken);
        }))
    }

    /// The list, opening upward over the composer as the model menu does.
    pub(crate) fn suggest_list(&self, p: Prompt, cx: &mut Context<Self>) -> Option<Div> {
        let list = self.suggestions(p, cx)?;
        let selected = self.suggest_state(p).selected.min(list.rows.len().saturating_sub(1));
        let pick = |i: usize| cx.listener(move |this, _: &ClickEvent, window, cx| _ = this.suggest_pick(p, i, window, cx));
        // Servers the agent can't use now follow those it can, and can't be selected.
        let r = |i: usize| row(("chat-suggest", i), i == selected && i < list.rows.len()).on_click(pick(i));
        let shown: Vec<Stateful<Div>> = match &list.rows {
            Rows::Commands { query, rows, .. } => window(selected, rows.len())
                .map(|i| {
                    let c = &rows[i];
                    r(i).child(div().flex_none().font_family(code_font()).child(marked(format!("/{}", c.name), &[query.to_lowercase()])))
                        .when(!c.hint.is_empty(), |r| r.child(div().flex_none().font_family(code_font()).text_size(px(12.)).text_color(TEXT_3).child(c.hint.clone())))
                        .child(div().flex_1().min_w_0().truncate().text_size(px(12.)).text_color(TEXT_3).child(c.description.clone()))
                })
                .collect(),
            Rows::Files { query, paths, .. } => window(selected, paths.len())
                .map(|i| {
                    let p = &paths[i];
                    r(i).child(file_icon(p, false, false, 14.)).child(div().flex_1().min_w_0().truncate().child(marked(p.clone(), &[query.to_lowercase()])))
                })
                .collect(),
            Rows::Servers { query, servers, .. } => {
                let provider = self.suggest_target(p).map(|t| t.provider).unwrap_or_default();
                let s = self.suggest_state(p);
                let acting = |name: &str| s.acting.get(&list.agent).filter(|(server, _)| server == name).map(|(_, a)| *a);
                match servers {
                    None => vec![note("Checking MCP servers…", TEXT_3)],
                    Some(Err(e)) => vec![note(e, FAILED_TEXT)],
                    Some(Ok(all)) if all.is_empty() => vec![note(if query.is_empty() { "No MCP servers found" } else { "No matching MCP servers" }, TEXT_3)],
                    Some(Ok(all)) => window(selected, all.len())
                        .map(|i| server_row(r(i), &all[i], &provider, query, s.signs_in(&list.agent, &all[i].name), acting(&all[i].name)))
                        .collect(),
                }
            }
        };
        let id = match p {
            Prompt::Chat => "chat-suggest",
            Prompt::Launch => "launch-suggest",
        };
        let pop = ui::pop(div())
            .id(id)
            .w(px(460.))
            .p(px(PAD))
            // It hangs over the transcript; without this a click on it also lands on what it covers.
            .occlude()
            .child(rows("chat-suggest-rows", shown))
            .on_mouse_down_out(cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                this.suggest_state_mut(p).list = None;
                cx.notify();
            }));
        Some(div().absolute().top(px(-6.)).left_0().child(deferred(anchored().anchor(Anchor::BottomLeft).snap_to_window_with_margin(px(8.)).child(pop)).with_priority(1)))
    }
}

#[cfg(test)]
mod tests {
    use super::{SHOWN, Suggest, launch_key, named, rank, replaced, starts_draft, window, word};
    use agents::{McpAction, McpServer, McpStatus};
    use std::collections::HashSet;

    fn owned(paths: &[&str]) -> Vec<String> {
        paths.iter().map(|p| (*p).to_string()).collect()
    }

    #[test]
    fn a_command_is_typed_from_a_slash_that_starts_a_word_to_its_first_space() {
        assert_eq!(word("/mo", 3, '/'), Some((0, "mo")));
        assert_eq!(word("/", 1, '/'), Some((0, "")));
        assert_eq!(word("/model haiku", 12, '/'), None);
        assert_eq!(word("/model haiku", 3, '/'), Some((0, "mo")));
        assert_eq!(word("see /mo", 7, '/'), Some((4, "mo")));
        assert_eq!(word("src/main", 8, '/'), None);
    }

    #[test]
    fn a_picked_command_keeps_the_argument_typed_after_it() {
        let (name, arg) = chat::slash("/mo  haiku").unwrap();
        assert_eq!(name, "mo");
        assert_eq!(named("model", arg), ("/model haiku".to_string(), "/model ".len()));
        assert_eq!(named("compact", ""), ("/compact ".to_string(), "/compact ".len()));
    }

    #[test]
    fn a_mention_starts_the_draft_or_follows_whitespace() {
        assert_eq!(word("@src", 4, '@'), Some((0, "src")));
        assert_eq!(word("look at @src/ma", 15, '@'), Some((8, "src/ma")));
        assert_eq!(word("line\n@", 6, '@'), Some((5, "")));
        assert_eq!(word("mail me@host", 12, '@'), None);
        assert_eq!(word("@src done", 9, '@'), None, "a space closes it");
        assert_eq!(word("tiếng @a", "tiếng @a".len(), '@'), Some(("tiếng ".len(), "a")));
    }

    #[test]
    fn files_the_agent_touched_come_first_then_changed_ones_then_the_rest() {
        let files = owned(&["a.rs", "b.rs", "c.rs", "d.rs", "readme.md"]);
        let ranked = rank("rs", &owned(&["d.rs", "c.rs"]), &HashSet::from(["b.rs"]), &files);
        assert_eq!(ranked, ["d.rs", "c.rs", "b.rs", "a.rs"]);
        assert_eq!(rank("", &[], &HashSet::new(), &files).len(), files.len());
        assert_eq!(rank("READ", &[], &HashSet::new(), &files), ["readme.md"]);
    }

    #[test]
    fn every_matching_file_is_ranked_so_the_list_scrolls_past_a_window() {
        let files: Vec<String> = (0..20).map(|i| format!("f{i}.rs")).collect();
        let changed = HashSet::from(["f19.rs"]);
        let ranked = rank("f", &[], &changed, &files);
        assert_eq!(ranked.len(), 20);
        assert_eq!(ranked[..3], ["f19.rs", "f0.rs", "f1.rs"]);
    }

    #[test]
    fn commands_are_asked_for_each_time_a_slash_starts_a_word_and_while_none_came() {
        let mut s = Suggest::default();
        assert!(s.asks_commands("a", "/", 1));
        assert!(!s.asks_commands("a", "/mo", 3), "asked already");
        assert!(s.asks_commands("a", "/", 1));
        assert!(s.asks_commands("b", "/mo", 3));
        assert!(s.asks_commands("a", "ok /", 4), "a slash past the start asks too");
        assert!(!s.asks_commands("c", "a/b", 3));
    }

    #[test]
    fn a_new_sessions_skills_are_kept_apart_by_provider_account_and_project() {
        let keys = [launch_key("claude", "", "/a"), launch_key("claude", "work", "/a"), launch_key("claude", "", "/b"), launch_key("codex", "", "/a")];
        assert_eq!(keys.iter().collect::<HashSet<_>>().len(), keys.len());
    }

    #[test]
    fn servers_are_asked_for_each_time_a_tag_starts_and_while_none_came() {
        let mut s = Suggest::default();
        assert!(s.asks_servers("a", "@mcp/", 5));
        assert!(!s.asks_servers("a", "@mcp/ff", 7), "asked already");
        assert!(s.asks_servers("a", "use @mcp/", 9));
        assert!(s.asks_servers("b", "use @mcp/ff", 11));
        assert!(!s.asks_servers("c", "@src/main", 9));
    }

    #[test]
    fn a_send_asks_the_agent_to_use_only_the_servers_it_told_of() {
        let mut s = Suggest::default();
        assert_eq!(s.with_mcp_context("a", "codex", "use @mcp/fff"), "use @mcp/fff", "none listed yet");
        s.mcp_listed("a".into(), Ok(vec![McpServer { name: "fff".into(), status: McpStatus::Connected, scope: String::new() }]), "", false);
        assert!(s.with_mcp_context("a", "codex", "use @mcp/fff").starts_with("MCP context: Use the configured server \"fff\" (codex)"));
        assert_eq!(s.with_mcp_context("b", "codex", "use @mcp/fff"), "use @mcp/fff", "another agent's servers");
    }

    #[test]
    fn a_sign_in_sent_to_the_browser_waits_until_the_server_no_longer_needs_one() {
        let server = |status| McpServer { name: "linear".into(), status, scope: String::new() };
        let mut s = Suggest::default();
        assert_eq!(s.act("a", "codex", &server(McpStatus::NeedsAuth)), Some(McpAction::Auth));
        s.mcp_listed("a".into(), Ok(vec![server(McpStatus::NeedsAuth)]), "https://auth.example", true);
        assert!(s.signs_in("a", "linear") && s.acting.is_empty());
        s.mcp_listed("a".into(), Ok(vec![server(McpStatus::NeedsAuth)]), "", false);
        assert!(s.signs_in("a", "linear"), "a list while the browser is open keeps it");
        s.mcp_listed("a".into(), Ok(vec![server(McpStatus::Connected)]), "", false);
        assert!(!s.signs_in("a", "linear"));
    }

    #[test]
    fn an_agent_acts_on_one_server_at_a_time_until_the_actions_own_reply() {
        let server = |name: &str| McpServer { name: name.into(), status: McpStatus::NeedsAuth, scope: String::new() };
        let mut s = Suggest::default();
        assert_eq!(s.act("a", "claude", &server("linear")), Some(McpAction::Auth));
        assert_eq!(s.act("a", "claude", &server("asana")), None);
        s.mcp_listed("a".into(), Ok(vec![server("linear")]), "", false);
        assert_eq!(s.act("a", "claude", &server("asana")), None, "a plain list crossing the reply ends nothing");
        s.mcp_listed("a".into(), Ok(vec![server("linear")]), "https://auth.example", true);
        assert!(s.signs_in("a", "linear"));
        assert_eq!(s.act("a", "claude", &server("asana")), Some(McpAction::Auth));
    }

    #[test]
    fn a_sign_in_that_needs_no_browser_waits_on_nothing() {
        let mut s = Suggest::default();
        s.acting.insert("a".into(), ("fff".into(), McpAction::Auth));
        s.mcp_listed("a".into(), Ok(vec![]), "", true);
        assert!(!s.signs_in("a", "fff") && s.acting.is_empty());
    }

    #[test]
    fn servers_are_asked_again_once_at_a_time_only_while_a_sign_in_or_a_connection_settles() {
        let server = |status| McpServer { name: "fff".into(), status, scope: String::new() };
        let mut s = Suggest::default();
        s.mcp_listed("a".into(), Ok(vec![server(McpStatus::Connected)]), "", false);
        assert!(!s.rechecks("a"));
        s.signing_in.insert(("a".into(), "linear".into()));
        assert!(s.rechecks("a"));
        assert!(!s.rechecks("a"), "one ask is due already");
        s.mcp_listed("b".into(), Ok(vec![server(McpStatus::Pending)]), "", false);
        assert!(s.rechecks("b"), "a server still connects");
    }

    #[test]
    fn files_are_listed_each_time_an_at_starts_a_mention_and_while_none_came() {
        let mut s = Suggest::default();
        assert!(s.loads_files("/r", "see @ma", 7));
        assert!(!s.loads_files("/r", "see @mai", 8), "listed already");
        assert!(s.loads_files("/r", "see @", 5));
        assert!(!s.loads_files("/r", "mail me@x", 9));
    }

    #[test]
    fn the_selection_wraps_at_either_end() {
        let mut s = Suggest::default();
        s.step(false, 3);
        assert_eq!(s.selected, 2);
        s.step(true, 3);
        assert_eq!(s.selected, 0);
        s.selected = 9;
        s.step(true, 3);
        assert_eq!(s.selected, 0, "a selection past a shrunk list starts from its last row");
    }

    #[test]
    fn the_window_scrolls_only_to_keep_the_selection_in_view() {
        assert_eq!(window(0, 20), 0..SHOWN);
        assert_eq!(window(SHOWN - 1, 20), 0..SHOWN);
        assert_eq!(window(SHOWN, 20), 1..SHOWN + 1);
        assert_eq!(window(19, 20), 20 - SHOWN..20);
        assert_eq!(window(2, 3), 0..3);
    }

    #[test]
    fn a_picked_file_replaces_the_mention_and_the_cursor_follows_it() {
        let (text, cursor) = replaced("fix @ma please", 4, 7, "@src/main.rs");
        assert_eq!(text, "fix @src/main.rs please");
        assert_eq!(cursor, "fix @src/main.rs ".len());
        assert_eq!(replaced("@ma", 0, 3, "@main.rs"), ("@main.rs ".to_string(), "@main.rs ".len()));
    }

    #[test]
    fn a_command_picked_past_the_drafts_start_replaces_only_its_word() {
        assert!(!starts_draft("ok /imp", 3));
        assert_eq!(replaced("ok /imp", 3, 7, "/implement"), ("ok /implement ".to_string(), "ok /implement ".len()));
        assert_eq!(replaced("ok /imp then", 3, 7, "/implement"), ("ok /implement then".to_string(), "ok /implement ".len()));
    }

    #[test]
    fn a_slash_after_only_whitespace_still_starts_the_draft() {
        assert!(starts_draft("/mo", 0));
        assert!(starts_draft(" \n/mo", 2));
        assert_eq!(chat::slash(" \n/model"), Some(("model", "")), "as a send takes it");
    }
}
