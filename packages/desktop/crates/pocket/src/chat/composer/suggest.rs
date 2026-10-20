use crate::chat::locate::touched;
use crate::composer::model_menu::{PAD, row, rows};
use crate::desktop::Desktop;
use crate::palette::{marked, matches};
use chat::{Kind, Slash};
use gpui_kit::component::input::{Enter, Escape, IndentInline, MoveDown, MoveUp, RopeExt as _, TextareaState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use theme::*;

/// The rows the list shows at once.
const SHOWN: usize = 8;

/// The `/` and `@` list over the composer, and what it lists from.
#[derive(Default)]
pub(crate) struct Suggest {
    /// Each agent's commands as its CLI lists them, by agent id; empty until they arrive.
    commands: HashMap<String, Vec<agents::Command>>,
    /// Each folder's files as git lists them, relative to it; empty while they load.
    files: HashMap<String, Vec<String>>,
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
}

impl Rows {
    fn len(&self) -> usize {
        match self {
            Rows::Commands { rows, .. } => rows.len(),
            Rows::Files { paths, .. } => paths.len(),
        }
    }
}

impl Suggest {
    pub(crate) fn listed(&mut self, agent: String, commands: Vec<agents::Command>) {
        self.commands.insert(agent, commands);
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

/// The composer a list serves.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Prompt {
    Chat,
    /// The new session's, whose agent doesn't run yet.
    Launch,
}

/// Who a draft goes to: the chat's agent, or the agent and project a new session starts; `key` is the agent's id or
/// the provider.
struct Target {
    key: String,
    provider: String,
    cwd: String,
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
            Prompt::Chat => self.chats.drafts.owner().and_then(|id| self.agents.get(id)).map(|a| Target { key: a.id.clone(), provider: a.provider.clone(), cwd: a.cwd.clone() }),
            Prompt::Launch => {
                let provider = self.new_form_pick().provider;
                self.new_form.project().map(|cwd| Target { key: provider.clone(), provider, cwd: cwd.into() })
            }
        }
    }

    /// Asks for what the list needs as the draft changes: the agent's commands each time a `/` starts one, and the
    /// folder's files each time an `@` does.
    pub(crate) fn draft_changed(&mut self, p: Prompt, cx: &mut Context<Self>) {
        self.suggest_state_mut(p).selected = 0;
        let Some(t) = self.suggest_target(p) else { return self.suggest(p, cx) };
        let input = self.suggest_input(p).read(cx);
        let (text, cursor) = (input.value().to_string(), input.cursor());
        let s = self.suggest_state_mut(p);
        if p == Prompt::Chat && s.asks_commands(&t.key, &text, cursor) {
            self.outbox.commands(&t.key);
        }
        if self.suggest_state_mut(p).loads_files(&t.cwd, &text, cursor) {
            self.load_files(p, &t.cwd, cx);
        }
        self.suggest(p, cx);
    }

    /// Works the list out again for the draft as it stands. A new session, or a `/` past the draft's start, lists
    /// only the commands sent as prompts: nothing runs yet, and a command runs only from the start.
    pub(crate) fn suggest(&mut self, p: Prompt, cx: &mut Context<Self>) {
        let t = self.suggest_target(p);
        let input = self.suggest_input(p).read(cx);
        let (text, cursor) = (input.value().to_string(), input.cursor());
        let s = self.suggest_state(p);
        let rows = t.as_ref().and_then(|t| {
            if let Some((at, query)) = word(&text, cursor, '/') {
                let listed = s.commands.get(&t.key).map_or(&[][..], Vec::as_slice);
                let mut rows = chat::menu(chat::for_provider(&t.provider), listed, query);
                if p == Prompt::Launch || !starts_draft(&text, at) {
                    rows.retain(|r| r.kind == Kind::Send);
                }
                return Some(Rows::Commands { at, query: query.into(), rows });
            }
            let (at, query) = word(&text, cursor, '@')?;
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

    /// The list, while the input holds the draft it was worked out for and it has rows.
    fn suggestions(&self, p: Prompt, cx: &App) -> Option<&List> {
        let list = self.suggest_state(p).list.as_ref()?;
        let current = self.suggest_target(p).is_some_and(|t| t.key == list.agent) && *self.suggest_input(p).read(cx).value() == *list.text;
        (current && list.rows.len() > 0).then_some(list)
    }

    fn suggest_move(&mut self, p: Prompt, down: bool, cx: &mut Context<Self>) -> bool {
        let Some(total) = self.suggestions(p, cx).map(|l| l.rows.len()) else { return false };
        self.suggest_state_mut(p).step(down, total);
        cx.notify();
        true
    }

    /// Puts the picked row in the draft: a command's name, ready for its argument, or a file's path. A command the
    /// desktop runs itself runs at once.
    fn suggest_pick(&mut self, p: Prompt, ix: usize, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.suggestions(p, cx).is_none() {
            return false;
        }
        let Some(list) = self.suggest_state_mut(p).list.take() else { return false };
        match list.rows {
            Rows::Commands { at, rows, .. } if !starts_draft(&list.text, at) => {
                let Some(c) = rows.get(ix) else { return true };
                let cursor = self.suggest_input(p).read(cx).cursor();
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
                        self.chat_local(&list.agent, local, arg, cx);
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
        }
        true
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
        let total = list.rows.len();
        let selected = self.suggest_state(p).selected.min(total - 1);
        let pick = |i: usize| cx.listener(move |this, _: &ClickEvent, window, cx| _ = this.suggest_pick(p, i, window, cx));
        let shown: Vec<Stateful<Div>> = window(selected, total)
            .map(|i| {
                let r = row(("chat-suggest", i), i == selected).on_click(pick(i));
                match &list.rows {
                    Rows::Commands { query, rows, .. } => {
                        let c = &rows[i];
                        r.child(div().flex_none().font_family(code_font()).child(marked(format!("/{}", c.name), &[query.to_lowercase()])))
                            .when(!c.hint.is_empty(), |r| r.child(div().flex_none().font_family(code_font()).text_size(px(12.)).text_color(TEXT_3).child(c.hint.clone())))
                            .child(div().flex_1().min_w_0().truncate().text_size(px(12.)).text_color(TEXT_3).child(c.description.clone()))
                    }
                    Rows::Files { query, paths, .. } => {
                        let p = &paths[i];
                        r.child(file_icon(p, false, false, 14.)).child(div().flex_1().min_w_0().truncate().child(marked(p.clone(), &[query.to_lowercase()])))
                    }
                }
            })
            .collect();
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
    use super::{SHOWN, Suggest, named, rank, replaced, starts_draft, window, word};
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
