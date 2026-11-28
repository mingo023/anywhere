mod composer;
mod menu;
mod selection;
pub(crate) mod toast;

use crate::actions::AddToChat;
use crate::chat::Route;
use crate::desktop::Desktop;
use crate::git_ui::diff::{DiffView, ordered, span};
use crate::modals::new_session::may_start;
use crate::region::notify_after_draw;
use crate::sidebar::in_tree;
use crate::status::{Card, Kind, Status};
use crate::syntax::language_for;
use crate::util::basename;
use gpui_kit::component::input::{InputEvent, InputState, TextareaState};
use gpui_kit::*;
use std::collections::HashMap;
use std::ops::Range;
use std::time::Duration;
use workspace::{Doc, Tab};

const ADDED_TOAST: Duration = Duration::from_millis(2600);
/// A return in the same write as the paste reads as part of the paste to some agents.
pub(crate) const SEND_AFTER: Duration = Duration::from_millis(80);

/// Text from a file or the chat, quoted into an agent's input.
#[derive(Clone, Debug, PartialEq)]
pub struct Quote {
    pub path: String,
    pub body: Body,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Body {
    /// `lines` count on the new side, unless the pick only removes lines (`removed`).
    Diff {
        lines: (usize, usize),
        removed: bool,
        text: String,
    },
    Code {
        lines: (usize, usize),
        language: &'static str,
        text: String,
    },
    Prose {
        section: Option<String>,
        text: String,
    },
    /// Picked from a chat's transcript; its `path` is empty.
    Chat {
        text: String,
    },
}

fn lines_text((lo, hi): (usize, usize)) -> String {
    if lo == hi { lo.to_string() } else { format!("{lo}-{hi}") }
}

impl Quote {
    pub fn code(path: &str, text: &str, range: Range<usize>) -> Option<Quote> {
        let selected = text.get(range.clone())?.trim_end_matches(['\n', '\r']);
        if selected.trim().is_empty() {
            return None;
        }
        let lo = text[..range.start].matches('\n').count() + 1;
        let lines = (lo, lo + selected.matches('\n').count());
        Some(Quote { path: path.into(), body: Body::Code { lines, language: language_of(path), text: selected.into() } })
    }

    /// Quotes `selected`, text rendered from the markdown `source`.
    pub fn prose(path: &str, source: &str, selected: &str) -> Option<Quote> {
        let text = selected.trim();
        if text.is_empty() {
            return None;
        }
        Some(Quote { path: path.into(), body: Body::Prose { section: section_of(source, text), text: text.into() } })
    }

    pub fn chat(selected: &str) -> Option<Quote> {
        let text = selected.trim();
        (!text.is_empty()).then(|| Quote { path: String::new(), body: Body::Chat { text: text.into() } })
    }

    pub fn payload(&self) -> String {
        match &self.body {
            Body::Diff { lines, removed, text } => {
                let side = if *removed { " (before)" } else { "" };
                format!("{}:{}{side}, uncommitted change\n```diff\n{text}\n```", self.path, lines_text(*lines))
            }
            Body::Code { lines, language, text } => format!("{}:{}\n```{language}\n{text}\n```", self.path, lines_text(*lines)),
            Body::Prose { section, text } => {
                let section = section.as_ref().map(|s| format!(" (section \"{s}\")")).unwrap_or_default();
                format!("{}{section}:\n{}", self.path, blockquote(text))
            }
            Body::Chat { text } => blockquote(text),
        }
    }

    pub fn label(&self) -> String {
        let name = basename(&self.path);
        match &self.body {
            Body::Diff { lines, .. } => format!("{name}:{} (diff)", lines_text(*lines)),
            Body::Code { lines, .. } => format!("{name}:{}", lines_text(*lines)),
            Body::Prose { section: Some(s), .. } => format!("{name} › {s}"),
            Body::Prose { section: None, .. } => name,
            Body::Chat { .. } => "Quote".into(),
        }
    }

    pub fn icon(&self) -> &'static str {
        match &self.body {
            Body::Diff { .. } => "diff-multiple",
            Body::Code { .. } | Body::Prose { .. } => "file",
            Body::Chat { .. } => "comment",
        }
    }

    pub fn excerpt(&self) -> String {
        let (Body::Diff { text, .. } | Body::Code { text, .. } | Body::Prose { text, .. } | Body::Chat { text }) = &self.body;
        text.lines().take(3).collect::<Vec<_>>().join("\n")
    }

    /// A new session's first prompt: the quote, then the question.
    pub fn prompt(&self, question: &str) -> String {
        match question.trim() {
            "" => self.payload(),
            q => format!("{}\n\n{q}", self.payload()),
        }
    }
}

fn language_of(path: &str) -> &'static str {
    match language_for(path) {
        "text" => "",
        l => l,
    }
}

/// The quote a sent prompt opens with, read back from what `payload` wrote, and what was typed after it.
pub(crate) fn split_quote(prompt: &str) -> Option<(Quote, &str)> {
    let (quote, rest) = if prompt.starts_with("> ") {
        let (text, rest) = unquote(prompt)?;
        (Quote { path: String::new(), body: Body::Chat { text } }, rest)
    } else {
        let (head, after) = prompt.split_once('\n')?;
        match head.strip_suffix(':') {
            Some(lead) => prose_quote(lead, after)?,
            None => fenced_quote(head, after)?,
        }
    };
    Some((quote, rest.trim_start_matches('\n')))
}

/// Every quote a sent prompt opens with, in order, and what was typed after them.
pub(crate) fn split_quotes(prompt: &str) -> (Vec<Quote>, &str) {
    let (mut quotes, mut rest) = (Vec::new(), prompt);
    while let Some((quote, after)) = split_quote(rest) {
        quotes.push(quote);
        rest = after;
    }
    (quotes, rest)
}

fn prose_quote<'a>(lead: &str, after: &'a str) -> Option<(Quote, &'a str)> {
    let (path, section) = match lead.strip_suffix("\")").and_then(|l| l.split_once(" (section \"")) {
        Some((path, section)) => (path, Some(section.to_string())),
        None => (lead, None),
    };
    if !names_a_file(path) {
        return None;
    }
    let (text, rest) = unquote(after)?;
    Some((Quote { path: path.into(), body: Body::Prose { section, text } }, rest))
}

fn unquote(s: &str) -> Option<(String, &str)> {
    let end = s.find("\n\n").unwrap_or(s.len());
    let lines: Option<Vec<&str>> = s[..end].lines().map(|l| if l == ">" { Some("") } else { l.strip_prefix("> ") }).collect();
    let text = lines?.join("\n");
    (!text.trim().is_empty()).then_some((text, &s[end..]))
}

fn fenced_quote<'a>(head: &str, after: &'a str) -> Option<(Quote, &'a str)> {
    let (lead, diff) = head.strip_suffix(", uncommitted change").map_or((head, false), |l| (l, true));
    let (lead, removed) = lead.strip_suffix(" (before)").map_or((lead, false), |l| (l, true));
    let (path, lines) = lead.rsplit_once(':')?;
    let lines: (usize, usize) = match lines.split_once('-') {
        Some((lo, hi)) => (lo.parse().ok()?, hi.parse().ok()?),
        None => (lines.parse().ok()?, lines.parse().ok()?),
    };
    let (fence, body) = after.split_once('\n')?;
    let language = fence.strip_prefix("```")?;
    let (text, rest) = if diff { fenced(body)? } else { counted(body, lines.1.checked_sub(lines.0)? + 1)? };
    let text = text.to_string();
    let body = match diff {
        true if language == "diff" => Body::Diff { lines, removed, text },
        true => return None,
        false => Body::Code { lines, language: language_of(path), text },
    };
    (!path.is_empty()).then(|| (Quote { path: path.into(), body }, rest))
}

fn fenced(body: &str) -> Option<(&str, &str)> {
    let mut from = 0;
    while let Some(at) = body[from..].find("\n```").map(|at| from + at) {
        let after = &body[at + 4..];
        if closes_quote(after) {
            return Some((&body[..at], after));
        }
        from = at + 4;
    }
    None
}

/// Code can hold a fence of its own, so its end is found by the line count in its header.
fn counted(body: &str, lines: usize) -> Option<(&str, &str)> {
    let end = body.match_indices('\n').nth(lines - 1).map(|(at, _)| at)?;
    let after = body[end..].strip_prefix("\n```")?;
    closes_quote(after).then(|| (&body[..end], after))
}

fn names_a_file(path: &str) -> bool {
    basename(path).contains('.')
}

fn closes_quote(after_fence: &str) -> bool {
    after_fence.is_empty() || after_fence.starts_with("\n\n")
}

pub(crate) fn blockquote(text: &str) -> String {
    text.lines().map(|l| if l.is_empty() { ">".to_string() } else { format!("> {l}") }).collect::<Vec<_>>().join("\n")
}

fn plain(s: &str) -> String {
    let kept: String = s.chars().filter(|c| !"*_`#>[]".contains(*c)).collect();
    kept.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The heading above `selected`'s first line in markdown `source`. The selection comes rendered, so both sides are compared without markup.
fn section_of(source: &str, selected: &str) -> Option<String> {
    let first = selected.lines().map(plain).find(|l| !l.is_empty())?;
    let (mut fenced, mut section) = (false, None);
    for line in source.lines() {
        let t = line.trim_start();
        if t.starts_with("```") {
            fenced = !fenced;
            continue;
        }
        if !fenced && t.starts_with('#') {
            section = Some(t.trim_start_matches('#').trim().to_string());
        }
        if plain(line).contains(&first) {
            return section;
        }
    }
    None
}

/// The quote pasted into a TUI with bracketed paste, then the question typed after it, with no return so nothing sends.
pub fn input_bytes(payload: &str, question: &str) -> Vec<u8> {
    let mut out = term::paste_bytes(payload, true);
    out.push(b' ');
    let question: String = question.split_whitespace().collect::<Vec<_>>().join(" ").chars().filter(|c| !c.is_control()).collect();
    out.extend(question.bytes());
    out
}

#[derive(Clone, Debug, PartialEq)]
pub struct Choice {
    pub id: String,
    pub provider: String,
    pub title: String,
    pub note: String,
    pub ready: bool,
}

pub fn choices(cards: Vec<Card>, branch: &str) -> Vec<Choice> {
    cards
        .into_iter()
        .filter(|c| c.kind == Kind::Agent)
        .map(|c| {
            let (note, ready) = match c.status {
                Status::NeedsYou => ("Answer its question first".to_string(), false),
                Status::Working => ("Working · waits in its input".to_string(), true),
                _ => (branch.to_string(), true),
            };
            Choice { id: c.id, provider: c.provider, title: c.title, note, ready }
        })
        .collect()
}

/// The agent picked, else the open one, else the first; each only while it can take a quote.
pub fn default_target(picked: Option<&str>, open: Option<&str>, choices: &[Choice]) -> Option<String> {
    let ready = |id: &&str| choices.iter().any(|c| c.id == *id && c.ready);
    picked.filter(ready).or(open.filter(ready)).map(str::to_string).or_else(|| choices.iter().find(|c| c.ready).map(|c| c.id.clone()))
}

/// The agent picked while it can take a quote; else, unless `to` is "ask", `focused` or `recent` as `to` says, then the first.
pub fn target_for(to: &str, picked: Option<&str>, focused: Option<&str>, recent: Option<&str>, choices: &[Choice]) -> Option<String> {
    match to {
        "ask" => picked.filter(|id| choices.iter().any(|c| c.id == *id && c.ready)).map(str::to_string),
        "recent" => default_target(picked, recent, choices),
        _ => default_target(picked, focused, choices),
    }
}

#[derive(Default, PartialEq)]
pub(crate) struct Side {
    pub choices: Vec<Choice>,
    pub target: Option<String>,
    pub names: HashMap<String, String>,
}

impl Side {
    fn agent_name(&self, provider: &str) -> String {
        self.names.get(provider).cloned().unwrap_or_else(|| theme::provider_name(provider).into())
    }
}

pub(crate) enum ChatEvent {
    Add,
    Cancel,
    NewSession,
    Asked,
    /// The menu closed on a press outside it, which may have left focus nowhere.
    MenuDismissed,
}

pub(crate) struct ChatComposer {
    pub(crate) input: Entity<TextareaState>,
    /// The agent picked in the menu; the default applies while it can't take a quote.
    pub(crate) target: Option<String>,
    pub(crate) menu: bool,
    /// The menu's filter, the row its arrows are on, and its list's scroll.
    pub(crate) search: Entity<InputState>,
    pub(crate) highlight: usize,
    pub(crate) scroll: ScrollHandle,
    /// Where the "Add to chat" pill shows, and the selection it quotes: pressing the pill clears a markdown selection.
    pub(crate) pill: Option<(Point<Pixels>, Quote)>,
    /// A file selection being asked about, and where its composer floats.
    pub(crate) file: Option<(Point<Pixels>, Quote)>,
    /// The provider last added to, confirmed in a toast until its timer hides it.
    added: Option<(String, Task<()>)>,
    side: Side,
    _subs: Vec<Subscription>,
}

impl EventEmitter<ChatEvent> for ChatComposer {}

impl ChatComposer {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| TextareaState::new(window, cx).placeholder("Ask about this selection…").rows(2));
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Send to…"));
        let subs = vec![
            cx.subscribe(&input, |_, _, ev: &InputEvent, cx| {
                if let InputEvent::PressEnter { secondary: true, .. } = ev {
                    cx.emit(ChatEvent::Add);
                }
            }),
            cx.subscribe(&search, |this, _, ev: &InputEvent, cx| {
                if let InputEvent::Change = ev {
                    this.reset_highlight(cx);
                    cx.notify();
                }
            }),
        ];
        Self {
            input,
            target: None,
            menu: false,
            search,
            highlight: 0,
            scroll: ScrollHandle::new(),
            pill: None,
            file: None,
            added: None,
            side: Side::default(),
            _subs: subs,
        }
    }

    pub(crate) fn hand(&mut self, side: Side, cx: &mut Context<Self>) {
        if self.side != side {
            self.side = side;
            notify_after_draw(cx);
        }
    }

    pub(crate) fn question(&self, cx: &App) -> String {
        self.input.read(cx).value().to_string()
    }

    pub(crate) fn file_quote(&self) -> Option<Quote> {
        self.file.as_ref().map(|(_, q)| q.clone())
    }

    pub(crate) fn added(&self) -> Option<&str> {
        self.added.as_ref().map(|(p, _)| p.as_str())
    }

    pub(crate) fn clear_added(&mut self) {
        self.added = None;
    }

    pub(crate) fn flash(&mut self, provider: String, cx: &mut Context<Self>) {
        let hide = cx.spawn(async |this, cx| {
            cx.background_executor().timer(ADDED_TOAST).await;
            this.update(cx, |c, cx| {
                c.added = None;
                cx.notify();
            })
            .ok();
        });
        self.added = Some((provider, hide));
    }

    pub(crate) fn reset(&mut self, forget_target: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.menu = false;
        (self.file, self.pill) = (None, None);
        if forget_target {
            self.target = None;
        }
        self.input.update(cx, |s, cx| s.set_value("", window, cx));
        cx.notify();
    }

    pub(crate) fn offer(&mut self, pill: Option<(Point<Pixels>, Quote)>, cx: &mut Context<Self>) {
        if pill != self.pill {
            self.pill = pill;
            cx.notify();
        }
    }

    pub(crate) fn clear_file(&mut self, cx: &mut Context<Self>) {
        self.file = None;
        cx.notify();
    }

    pub(crate) fn set_file(&mut self, file: Option<(Point<Pixels>, Quote)>, cx: &mut Context<Self>) {
        self.file = file;
        cx.notify();
    }

    pub(crate) fn drop_pill(&mut self) -> bool {
        self.pill.take().is_some()
    }

    pub(crate) fn ask(&mut self, at: Point<Pixels>, quote: Quote, window: &mut Window, cx: &mut Context<Self>) {
        (self.file, self.menu) = (Some((at, quote)), false);
        self.input.update(cx, |s, cx| {
            s.set_value("", window, cx);
            s.focus(window, cx);
        });
        cx.emit(ChatEvent::Asked);
        cx.notify();
    }

    pub(crate) fn focus_input(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.input.update(cx, |s, cx| s.focus(window, cx));
    }

    pub(crate) fn search_focused(&self, window: &Window, cx: &App) -> bool {
        self.search.focus_handle(cx).is_focused(window)
    }
}

impl Desktop {
    pub(crate) fn add_to_chat(&mut self, _: &AddToChat, window: &mut Window, cx: &mut Context<Self>) {
        let pane = self.focused_pane(cx);
        match self.pane_tab(pane, cx) {
            Some(Tab::Doc(Doc::Diff(_) | Doc::CompareFile { .. })) => {
                let quotable = |v: &&DiffView| v.followed_file().is_some() && v.pick.range.is_some_and(|r| span(&v.lines, ordered(r)).is_some());
                if let Some(i) = self.diff.view(pane, cx).filter(quotable).and_then(|v| v.pick.last()) {
                    self.open_composer(pane, i, window, cx);
                }
            }
            Some(Tab::Doc(Doc::File(_))) if self.chat.read(cx).file.is_none() => self.ask_about_text(pane, window, cx),
            Some(Tab::Term(terminal)) if self.chat_shown(&terminal) => self.quote_chat_selection(&terminal, window, cx),
            _ => {}
        }
    }

    pub(crate) fn chat_choices(&self, cx: &App) -> Vec<Choice> {
        let (Some(project), Some(tree)) = (self.project.as_deref(), self.place(cx)) else { return Vec::new() };
        let branch = self.repo(cx).map(|r| r.branch.clone()).unwrap_or_default();
        choices(in_tree(self.cards(project, cx), Some(&tree), |c| self.tree_of(c, cx)), &branch)
    }

    pub(crate) fn chat_target(&self, choices: &[Choice], cx: &App) -> Option<String> {
        let recent = choices.iter().filter(|c| c.ready).filter_map(|c| self.agents.get(&c.id)).max_by_key(|a| a.updated_at).map(|a| a.id.as_str());
        target_for(self.store.agents.chat_to(), self.chat.read(cx).target.as_deref(), self.session(), recent, choices)
    }

    pub(crate) fn chat_side(&self, cx: &App) -> Side {
        let choices = self.chat_choices(cx);
        Side { target: self.chat_target(&choices, cx), names: choices.iter().map(|c| (c.provider.clone(), self.agent_name(&c.provider))).collect(), choices }
    }

    fn chat_quote(&self, cx: &App) -> Option<Quote> {
        self.chat.read(cx).file_quote().or_else(|| self.diff.draft_quote(cx))
    }

    pub(crate) fn on_chat_event(&mut self, _: &Entity<ChatComposer>, ev: &ChatEvent, window: &mut Window, cx: &mut Context<Self>) {
        match ev {
            ChatEvent::Add => self.add_to_input(window, cx),
            ChatEvent::Cancel => self.cancel_chat(window, cx),
            ChatEvent::NewSession => self.chat_in_new_session(window, cx),
            ChatEvent::Asked => self.diff.cancel_draft(cx),
            ChatEvent::MenuDismissed => {
                // The press then focuses what it lands on; take focus back unless that's something else, like a terminal.
                cx.defer_in(window, |this, window, cx| {
                    let stayed = this.root.is_focused(window) || this.chat.read(cx).search_focused(window, cx);
                    if stayed && this.chat_quote(cx).is_some() {
                        this.chat.update(cx, |c, cx| c.focus_input(window, cx));
                    }
                });
            }
        }
    }

    pub fn add_to_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.agents.observe_only() {
            return;
        }
        let (Some(quote), Some(target)) = (self.chat_quote(cx), self.chat_target(&self.chat_choices(cx), cx)) else { return };
        let Some((terminal, provider)) = self.agents.get(&target).map(|a| (a.terminal_id.clone(), a.provider.clone())) else { return };
        let question = self.chat.read(cx).question(cx);
        match self.route(&terminal, cx) {
            Route::Chat if self.store.agents.send_now => self.chat_command(&target, chat::Command::Send(quote.prompt(&question)), cx),
            Route::Chat => {
                let text = self.chat_draft_edit(&target, cx, |d| d.add_quote(&quote, &question));
                if !text.is_empty() {
                    self.chat_add_text(&target, &text, window, cx);
                }
                cx.notify();
            }
            Route::Paste => {
                self.send_input(&terminal, &input_bytes(&quote.payload(), &question), cx);
                if self.store.agents.send_now {
                    cx.spawn(async move |this, cx| {
                        cx.background_executor().timer(SEND_AFTER).await;
                        this.update(cx, |d, cx| d.send_input(&terminal, b"\r", cx))
                    })
                    .detach();
                }
            }
            Route::Blocked => {
                self.chat.update(cx, |c, _| c.clear_added());
                self.fail("This agent can't take pasted text right now.", cx);
                return cx.notify();
            }
        }
        self.cancel_chat(window, cx);
        self.flash_added(provider, cx);
    }

    pub(crate) fn flash_added(&mut self, provider: String, cx: &mut Context<Self>) {
        self.chat.update(cx, |c, cx| c.flash(provider, cx));
    }

    pub fn cancel_chat(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.diff.cancel_draft(cx);
        let ask = self.store.agents.chat_to() == "ask";
        self.chat.update(cx, |c, cx| c.reset(ask, window, cx));
        window.focus(&self.root, cx);
        cx.notify();
    }

    fn chat_in_new_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(quote) = self.chat_quote(cx) else { return };
        let question = self.chat.read(cx).question(cx);
        self.cancel_chat(window, cx);
        if !may_start(&self.agents) {
            return;
        }
        // After `new_session`, which clears a draft it doesn't keep.
        self.new_session(Some(question.trim().to_string()), false, window, cx);
        self.new_form.update(cx, |f, cx| {
            f.composer.update(cx, |c, cx| {
                c.draft.add_quote(&quote, "");
                cx.notify();
            })
        });
    }
}

#[cfg(test)]
mod tests {
    use super::{Body, Choice, Quote, choices, default_target, input_bytes, split_quote, split_quotes, target_for};
    use crate::status::{Card, Kind, Status};

    fn diff(lines: (usize, usize), removed: bool, text: &str) -> Quote {
        Quote { path: "src/a.rs".into(), body: Body::Diff { lines, removed, text: text.into() } }
    }

    #[test]
    fn a_diff_quote_names_its_new_lines_and_fences_the_change() {
        assert_eq!(diff((27, 31), false, "+x\n-y").payload(), "src/a.rs:27-31, uncommitted change\n```diff\n+x\n-y\n```");
    }

    #[test]
    fn a_diff_quote_of_removed_lines_alone_says_they_are_the_old_ones() {
        assert_eq!(diff((4, 4), true, "-y").payload(), "src/a.rs:4 (before), uncommitted change\n```diff\n-y\n```");
    }

    #[test]
    fn labels_a_quote_by_file_name_and_lines() {
        assert_eq!(diff((27, 31), false, "").label(), "a.rs:27-31 (diff)");
        assert_eq!(diff((4, 4), true, "").label(), "a.rs:4 (diff)");
    }

    #[test]
    fn a_new_sessions_prompt_puts_the_question_after_the_quote() {
        let q = diff((4, 4), false, "+y");
        assert_eq!(q.prompt("  why?  "), format!("{}\n\nwhy?", q.payload()));
        assert_eq!(q.prompt(" "), q.payload());
    }

    #[test]
    fn input_pastes_the_quote_then_types_the_question_without_a_return() {
        assert_eq!(input_bytes("a\nb", "why\n this?"), b"\x1b[200~a\nb\x1b[201~ why this?");
    }

    #[test]
    fn input_strips_control_characters_from_the_question() {
        assert_eq!(input_bytes("a", "x\x1b[2Jy\r"), b"\x1b[200~a\x1b[201~ x[2Jy");
    }

    fn card(id: &str, status: Status, kind: Kind) -> Card {
        Card {
            id: id.into(),
            provider: "claude".into(),
            model: String::new(),
            title: format!("{id} title"),
            cwd: "/p".into(),
            local: String::new(),
            at: 0,
            created: 0,
            status,
            kind,
            notice: None,
            pinned: false,
            account: String::new(),
            child: false,
        }
    }

    fn choice(id: &str, note: &str, ready: bool) -> Choice {
        Choice { id: id.into(), provider: "claude".into(), title: format!("{id} title"), note: note.into(), ready }
    }

    #[test]
    fn an_agent_asking_a_question_cant_take_a_quote() {
        let cards = vec![card("a", Status::NeedsYou, Kind::Agent), card("b", Status::Working, Kind::Agent), card("c", Status::Idle, Kind::Agent)];
        assert_eq!(
            choices(cards, "main"),
            vec![choice("a", "Answer its question first", false), choice("b", "Working · waits in its input", true), choice("c", "main", true)]
        );
    }

    #[test]
    fn terminals_without_an_agent_are_not_offered() {
        assert_eq!(choices(vec![card("a", Status::Idle, Kind::NotAttached)], "main"), vec![]);
    }

    #[test]
    fn a_quote_goes_to_the_picked_agent_while_it_can_take_it() {
        let list = [choice("a", "", true), choice("b", "", true)];
        assert_eq!(default_target(Some("b"), Some("a"), &list).as_deref(), Some("b"));
    }

    #[test]
    fn a_quote_falls_back_to_the_open_agent_then_the_first_that_can_take_it() {
        let list = [choice("busy", "", false), choice("a", "", true), choice("b", "", true)];
        assert_eq!(default_target(Some("busy"), Some("b"), &list).as_deref(), Some("b"));
        assert_eq!(default_target(Some("gone"), Some("busy"), &list).as_deref(), Some("a"));
        assert_eq!(default_target(None, None, &[choice("busy", "", false)]), None);
    }

    #[test]
    fn a_quote_goes_to_the_focused_or_most_recent_agent_or_waits_for_a_pick() {
        let list = [choice("a", "", true), choice("b", "", true), choice("busy", "", false)];
        assert_eq!(target_for("focused", None, Some("a"), Some("b"), &list).as_deref(), Some("a"));
        assert_eq!(target_for("recent", None, Some("a"), Some("b"), &list).as_deref(), Some("b"));
        assert_eq!(target_for("ask", None, Some("a"), Some("b"), &list), None);
        assert_eq!(target_for("ask", Some("b"), Some("a"), None, &list).as_deref(), Some("b"));
        assert_eq!(target_for("ask", Some("busy"), None, None, &list), None);
    }

    #[test]
    fn a_code_quote_numbers_the_lines_it_touches_and_fences_them_in_their_language() {
        let q = Quote::code("src/a.rs", "a\nb\nc\nd\n", 2..6).unwrap();
        assert_eq!(q.payload(), "src/a.rs:2-3\n```rust\nb\nc\n```");
        assert_eq!(q.label(), "a.rs:2-3");
        assert_eq!(Quote::code("src/a.rs", "a\nb\n", 2..3).unwrap().payload(), "src/a.rs:2\n```rust\nb\n```");
    }

    #[test]
    fn a_code_quote_of_a_framework_file_fences_it_in_the_name_the_chat_colours() {
        assert_eq!(Quote::code("src/App.vue", "<template />", 0..12).unwrap().payload(), "src/App.vue:1\n```vue\n<template />\n```");
    }

    #[test]
    fn a_code_quote_of_an_unknown_language_has_a_bare_fence() {
        assert_eq!(Quote::code("notes.txt", "hi", 0..2).unwrap().payload(), "notes.txt:1\n```\nhi\n```");
    }

    #[test]
    fn an_empty_or_blank_code_selection_quotes_nothing() {
        assert_eq!(Quote::code("a.rs", "a\nb", 2..2), None);
        assert_eq!(Quote::code("a.rs", "a\n \nb", 2..4), None);
    }

    #[test]
    fn a_prose_quote_marks_each_line_and_names_its_section() {
        let source = "# Plan\n\n## Rollout\n\nShip it **slowly**.\n\nThen watch.\n";
        let q = Quote::prose("docs/PLAN.md", source, "Ship it slowly.\n\nThen watch.\n").unwrap();
        assert_eq!(q.payload(), "docs/PLAN.md (section \"Rollout\"):\n> Ship it slowly.\n>\n> Then watch.");
        assert_eq!(q.label(), "PLAN.md › Rollout");
    }

    #[test]
    fn a_section_ignores_hashes_inside_code_fences() {
        let source = "## Setup\n```sh\n# install\nnpm i\n```\n";
        assert_eq!(Quote::prose("README.md", source, "npm i").unwrap().label(), "README.md › Setup");
    }

    #[test]
    fn prose_above_any_heading_has_no_section() {
        let q = Quote::prose("README.md", "Intro text.\n# A\n", "Intro text.").unwrap();
        assert_eq!(q.payload(), "README.md:\n> Intro text.");
        assert_eq!(q.label(), "README.md");
        assert_eq!(Quote::prose("README.md", "x", "  \n"), None);
    }

    #[test]
    fn a_sent_quote_reads_back_with_what_was_typed_after_it() {
        let quotes = [
            Quote::code("src/a.rs", "a\nb\nc\n", 2..5).unwrap(),
            Quote::code("notes.txt", "hi", 0..2).unwrap(),
            diff((27, 31), false, "+x\n-y"),
            diff((4, 4), true, "-y"),
            Quote::prose("docs/PLAN.md", "# Rollout\nShip it slowly.\n\nThen watch.", "Ship it slowly.\n\nThen watch.").unwrap(),
            Quote::prose("README.md", "Intro text.", "Intro text.").unwrap(),
            Quote::chat(" builds, running fmt\n\n  then clippy ").unwrap(),
        ];
        for q in quotes {
            assert_eq!(split_quote(&q.prompt("why?\n\nand this")), Some((q.clone(), "why?\n\nand this")));
            assert_eq!(split_quote(&q.prompt("")), Some((q, "")));
        }
    }

    #[test]
    fn a_quoted_fence_inside_the_code_stays_in_the_code() {
        let q = Quote::code("README.md", "```sh\nnpm i\n```\n", 0..16).unwrap();
        assert_eq!(split_quote(&q.prompt("ok")), Some((q, "ok")));
    }

    #[test]
    fn a_quoted_fence_followed_by_a_blank_line_stays_in_the_code() {
        let source = "```sh\nnpm i\n```\n\nThen run it.\n";
        let q = Quote::code("README.md", source, 0..source.len()).unwrap();
        assert_eq!(split_quote(&q.prompt("ok")), Some((q, "ok")));
    }

    #[test]
    fn a_prompt_that_doesnt_open_with_a_quote_has_none() {
        assert_eq!(split_quote("Fix CI"), None);
        assert_eq!(split_quotes("Fix CI"), (vec![], "Fix CI"));
        assert_eq!(split_quote("Note:\n> keep this"), None);
        assert_eq!(split_quote("see src/a.rs:12\nthen fix it"), None);
        assert_eq!(split_quote("src/a.rs:12\n```rust\nlet a = 1;"), None);
        assert_eq!(split_quote("src/a.rs:1, uncommitted change\n```rust\nx\n```"), None);
    }

    #[test]
    fn a_chat_quote_marks_each_line_and_names_no_file() {
        let q = Quote::chat("  Ship it.\n\nThen watch.\n").unwrap();
        assert_eq!(q.payload(), "> Ship it.\n>\n> Then watch.");
        assert_eq!(q.label(), "Quote");
        assert_eq!(Quote::chat(" \n"), None);
    }

    #[test]
    fn a_prompt_opening_with_a_blockquote_reads_back_as_a_chat_quote() {
        assert_eq!(split_quote("> a\n> b\n\nwhy?"), Some((Quote::chat("a\nb").unwrap(), "why?")));
        assert_eq!(split_quote("> a"), Some((Quote::chat("a").unwrap(), "")));
        assert_eq!(split_quote(">no space\n\nwhy?"), None);
        assert_eq!(split_quote("> a\nb\n\nwhy?"), None);
    }

    #[test]
    fn an_excerpt_shows_the_first_three_lines() {
        assert_eq!(Quote::code("a.rs", "1\n2\n3\n4\n", 0..8).unwrap().excerpt(), "1\n2\n3");
    }

    #[test]
    fn several_quotes_read_back_in_order_before_the_question() {
        let a = Quote::code("src/a.rs", "let a = 1;\n", 0..10).unwrap();
        let b = Quote::prose("docs/b.md", "# Why\n\nBecause.", "Because.").unwrap();
        let c = Quote::chat("builds, running fmt").unwrap();
        let prompt = format!("{}\n\n{}\n\n{}\n\nwhich one?", a.payload(), c.payload(), b.payload());
        assert_eq!(split_quotes(&prompt), (vec![a, c, b], "which one?"));
    }
}
