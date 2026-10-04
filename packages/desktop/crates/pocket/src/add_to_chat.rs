mod composer;
mod menu;
mod selection;
mod toast;

pub(crate) use selection::offers_chat;

use crate::actions::AddToChat;
use crate::desktop::Desktop;
use crate::desktop::chrome::Overlay;
use crate::git_ui::diff::{DiffView, ordered, span};
use crate::sidebar::in_tree;
use crate::status::{Card, Kind, Status};
use crate::syntax::language_for;
use crate::util::basename;
use gpui_kit::component::input::{InputEvent, TextareaState};
use gpui_kit::*;
use std::ops::Range;
use std::time::Duration;
use workspace::tree::PaneId;
use workspace::{Doc, Tab};

const ADDED_TOAST: Duration = Duration::from_millis(2600);

/// Text from a file, quoted into an agent's input.
#[derive(Clone, Debug, PartialEq)]
pub struct Quote {
    pub path: String,
    pub body: Body,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Body {
    /// `lines` count on the new side, unless the pick only removes lines (`removed`).
    Diff { lines: (usize, usize), removed: bool, text: String },
    Code { lines: (usize, usize), language: &'static str, text: String },
    Prose { section: Option<String>, text: String },
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
        let language = match language_for(path) {
            "text" => "",
            l => l,
        };
        Some(Quote { path: path.into(), body: Body::Code { lines, language, text: selected.into() } })
    }

    /// Quotes `selected`, text rendered from the markdown `source`.
    pub fn prose(path: &str, source: &str, selected: &str) -> Option<Quote> {
        let text = selected.trim();
        if text.is_empty() {
            return None;
        }
        Some(Quote { path: path.into(), body: Body::Prose { section: section_of(source, text), text: text.into() } })
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
                let quoted: Vec<String> = text.lines().map(|l| if l.is_empty() { ">".into() } else { format!("> {l}") }).collect();
                format!("{}{section}:\n{}", self.path, quoted.join("\n"))
            }
        }
    }

    pub fn label(&self) -> String {
        let name = basename(&self.path);
        match &self.body {
            Body::Diff { lines, .. } => format!("{name}:{} (diff)", lines_text(*lines)),
            Body::Code { lines, .. } => format!("{name}:{}", lines_text(*lines)),
            Body::Prose { section: Some(s), .. } => format!("{name} › {s}"),
            Body::Prose { section: None, .. } => name,
        }
    }

    pub fn icon(&self) -> &'static str {
        match &self.body {
            Body::Diff { .. } => "diff-multiple",
            Body::Code { .. } | Body::Prose { .. } => "file",
        }
    }

    pub fn excerpt(&self) -> String {
        let (Body::Diff { text, .. } | Body::Code { text, .. } | Body::Prose { text, .. }) = &self.body;
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

/// The quote pasted, then the question typed after it, with no return so nothing sends.
/// None without bracketed paste, where the quote's newlines would send it.
pub fn input_bytes(payload: &str, question: &str, bracketed: bool) -> Option<Vec<u8>> {
    if !bracketed {
        return None;
    }
    let mut out = term::paste_bytes(payload, true);
    out.push(b' ');
    let question: String = question.split_whitespace().collect::<Vec<_>>().join(" ").chars().filter(|c| !c.is_control()).collect();
    out.extend(question.bytes());
    Some(out)
}

#[derive(Debug, PartialEq)]
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

pub struct ChatComposer {
    pub(crate) input: Entity<TextareaState>,
    /// The agent picked in the menu; the default applies while it can't take a quote.
    pub(crate) target: Option<String>,
    pub(crate) menu: bool,
    /// The pane whose file has text selected, and where the "Add to chat" pill shows.
    pub(crate) pill: Option<(PaneId, Point<Pixels>)>,
    /// A file selection being asked about, and where its composer floats.
    pub(crate) file: Option<(Point<Pixels>, Quote)>,
    /// The provider last added to, confirmed in a toast until its timer hides it.
    pub(crate) added: Option<(String, Task<()>)>,
}

impl ChatComposer {
    pub fn new(window: &mut Window, cx: &mut Context<Desktop>) -> (Self, Vec<Subscription>) {
        let input = cx.new(|cx| TextareaState::new(window, cx).placeholder("Ask about this selection…").rows(2));
        let subs = vec![cx.subscribe_in(&input, window, |this, _, ev: &InputEvent, window, cx| {
            if let InputEvent::PressEnter { secondary: true, .. } = ev {
                this.add_to_input(window, cx);
            }
        })];
        (Self { input, target: None, menu: false, pill: None, file: None, added: None }, subs)
    }
}

impl Desktop {
    pub(crate) fn add_to_chat(&mut self, _: &AddToChat, window: &mut Window, cx: &mut Context<Self>) {
        let pane = self.focused_pane();
        match self.pane_tab(pane) {
            Some(Tab::Doc(Doc::Diff(_))) => {
                let quotable = |v: &&DiffView| v.at.is_none() && v.pick.range.is_some_and(|r| span(&v.lines, ordered(r)).is_some());
                if let Some(i) = self.diff.view(pane).filter(quotable).and_then(|v| v.pick.last()) {
                    self.open_composer(pane, i, window, cx);
                }
            }
            Some(Tab::Doc(Doc::File(_))) if self.chat.file.is_none() => self.ask_about_text(pane, window, cx),
            _ => {}
        }
    }

    fn chat_choices(&self) -> Vec<Choice> {
        let (Some(project), Some(tree)) = (self.project.as_deref(), self.cwd()) else { return Vec::new() };
        let branch = self.repo().map(|r| r.branch.clone()).unwrap_or_default();
        choices(in_tree(self.cards(project), Some(&tree), |c| self.tree_of(c)), &branch)
    }

    fn chat_target(&self, choices: &[Choice]) -> Option<String> {
        default_target(self.chat.target.as_deref(), self.session.as_deref(), choices)
    }

    fn chat_quote(&self) -> Option<Quote> {
        self.chat.file.as_ref().map(|(_, q)| q.clone()).or_else(|| self.diff.draft_quote())
    }

    pub fn add_to_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.agents.observe_only() {
            return;
        }
        let (Some(quote), Some(target)) = (self.chat_quote(), self.chat_target(&self.chat_choices())) else { return };
        let Some((terminal, provider)) = self.agents.get(&target).map(|a| (a.terminal_id.clone(), a.provider.clone())) else { return };
        let Some(bracketed) = self.terminals.sessions.term(&terminal).map(|t| t.mode(2004)) else { return };
        let question = self.chat.input.read(cx).value();
        match input_bytes(&quote.payload(), &question, bracketed) {
            Some(bytes) => {
                self.send_input(&terminal, &bytes, cx);
                self.cancel_chat(window, cx);
                self.flash_added(provider, cx);
            }
            None => {
                self.chat.added = None;
                self.error = Some("This agent can't take pasted text right now.".into());
                cx.notify();
            }
        }
    }

    fn flash_added(&mut self, provider: String, cx: &mut Context<Self>) {
        let hide = cx.spawn(async |this, cx| {
            cx.background_executor().timer(ADDED_TOAST).await;
            this.update(cx, |d, cx| {
                d.chat.added = None;
                cx.notify();
            })
            .ok();
        });
        self.chat.added = Some((provider, hide));
    }

    pub fn cancel_chat(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.diff.cancel_draft();
        self.chat.menu = false;
        (self.chat.file, self.chat.pill) = (None, None);
        self.chat.input.update(cx, |s, cx| s.set_value("", window, cx));
        window.focus(&self.root, cx);
        cx.notify();
    }

    fn chat_in_new_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(quote) = self.chat_quote() else { return };
        let prompt = quote.prompt(&self.chat.input.read(cx).value());
        self.cancel_chat(window, cx);
        self.open(Overlay::NewSession, window, cx);
        if self.overlay == Some(Overlay::NewSession) {
            self.reset_new_form(Some(prompt), false, window, cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Body, Choice, Quote, choices, default_target, input_bytes};
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
        assert_eq!(input_bytes("a\nb", "why\n this?", true).unwrap(), b"\x1b[200~a\nb\x1b[201~ why this?");
    }

    #[test]
    fn input_strips_control_characters_from_the_question() {
        assert_eq!(input_bytes("a", "x\x1b[2Jy\r", true).unwrap(), b"\x1b[200~a\x1b[201~ x[2Jy");
    }

    #[test]
    fn input_is_refused_where_newlines_would_send_the_quote() {
        assert_eq!(input_bytes("a\nb", "why?", false), None);
    }

    fn card(id: &str, status: Status, kind: Kind) -> Card {
        Card { id: id.into(), provider: "claude".into(), agent: String::new(), title: format!("{id} title"), cwd: "/p".into(), at: 0, created: 0, status, kind, notice: None, pinned: false }
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
    fn a_code_quote_numbers_the_lines_it_touches_and_fences_them_in_their_language() {
        let q = Quote::code("src/a.rs", "a\nb\nc\nd\n", 2..6).unwrap();
        assert_eq!(q.payload(), "src/a.rs:2-3\n```rust\nb\nc\n```");
        assert_eq!(q.label(), "a.rs:2-3");
        assert_eq!(Quote::code("src/a.rs", "a\nb\n", 2..3).unwrap().payload(), "src/a.rs:2\n```rust\nb\n```");
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
    fn an_excerpt_shows_the_first_three_lines() {
        assert_eq!(Quote::code("a.rs", "1\n2\n3\n4\n", 0..8).unwrap().excerpt(), "1\n2\n3");
    }
}
