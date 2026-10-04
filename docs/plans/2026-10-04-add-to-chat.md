# Add to Chat Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use /implement to execute this plan task-by-task.

**Goal:** Select lines in a diff, or text in a code or markdown preview, and add them, plus an optional question, to an agent's terminal input. Nothing is submitted; the user presses Enter in the agent.

**Architecture:** A new `pocket/src/add_to_chat.rs` feature module owns:
- the `Quote` value type (where the text came from and what it says), with plain functions that build the payload and the input bytes;
- the agent choices and default-target rules;
- `ChatComposer` state (question textarea, picked agent, menu, pill, floating file composer);
- its views: `add_to_chat/composer.rs`, `add_to_chat/menu.rs` and, in PR2, `add_to_chat/selection.rs`.

Text reaches the agent through the existing `Desktop::send_input` as a bracketed paste (`term::paste_bytes`) followed by the typed question with no `\r`, so pocketd needs no change. PR1 replaces the diff's comment flow (composer, sent cards, Resolve, notes) with this composer. PR2 adds the selection pill and floating composer for code and markdown previews, plus ⌘L.

**Toolset** (run everything from `/Users/mingo/Developer/self/coding-pocket/packages/desktop`):
- Tests by name filter: `cargo test -p pocket <filter>`
- Gate before calling a PR done: `cargo build --workspace && cargo clippy --workspace --all-targets && cargo test --workspace`
- Record the clippy warning count before Task 1.1 (`cargo clippy --workspace --all-targets 2>&1 | grep -c '^warning'`). It must not grow.
- Screens (from the repo root): `.ui-review/fixture/capture.sh <dir> <name>=<step>,<step>…`
- Do NOT run `cargo fmt`. There is no rustfmt.toml, and lines run to ~200 chars by hand.
- Do not commit. Do not stash.
- No comments unless the WHY can't be read from the code (see `~/.claude/CLAUDE.md` §4). Docblocks only where they add a fact the signature doesn't.

**Read first:**
- `CLAUDE.md` (repo root): layout rules (feature module with its own state struct; thin `impl Desktop`; tests beside the logic, named as sentences; no mocks or render tests) and performance rules.
- `docs/adr/0003-desktop-code-layout.md`: the module map you update in Task 1.5.
- `.ui-review/prototypes/add-to-chat.html`: the approved UI spec (pill, composer card, "Add to" menu).
- `packages/desktop/crates/pocket/src/git_ui/diff.rs`: `Pick`, `span`, `DiffState`, and the comment flow this replaces.
- `packages/desktop/crates/pocket/src/git_ui/diff/composer.rs` and `target.rs`: the card and menu the new ones mirror (both deleted in Task 1.4).
- `packages/desktop/crates/pocket/src/terminal_view.rs:173-190`: `offer_paste` / `paste_into` / `send_input`, which show how pastes reach a terminal.

**Deliberate limits** (agreed; don't add):
- Adding keeps you where you are; a toast confirms it.
- One "New session…" item; the new-session form has its own provider picker.
- No keyboard navigation in the menu.
- The preview selection isn't cleared after adding.
- The pill doesn't follow scrolling.
- Agents are limited to the worktree on screen.

---

## PR 1: Diff picks add to the agent's input

**Scope:** The new `add_to_chat` module (quotes, choices, composer, menu). A diff pick now opens that composer, and "Add to input" pastes the quote into the agent without sending it. Diff comments are removed (sent cards, Resolve, the Changes panel notes and badges). File previews don't change yet.
**Depends on:** nothing
**Done when:** the gate is green; picking diff lines and pressing ⌘↵ puts `path:lo-hi, uncommitted change` plus a fenced diff into the agent's input, unsent.

### Task 1.1: Quotes, input bytes and agent choices

**Files:**
- Create: `packages/desktop/crates/pocket/src/add_to_chat.rs`
- Modify: `packages/desktop/crates/pocket/src/main.rs` (module list, lines 1-17)

**Context:**
- `Card` (`status.rs:51`) is one agent: `id, provider, title, cwd, at, created, status: Status, kind: Kind, notice`.
  - `Kind::Agent` is a running agent.
  - `Kind::NotAttached` is a terminal whose agent isn't attached; those can't take a quote.
  - `Status::NeedsYou` means the agent is asking the user something. Pasting would answer its prompt, so it's offered but disabled.
- `term::paste_bytes(text, true)` wraps `text` in `\x1b[200~ … \x1b[201~` and blanks control bytes except newlines. Agents (Claude Code, Codex) enable bracketed paste, so the newlines stay in their input instead of submitting.
- Without bracketed paste, `paste_bytes` turns `\n` into `\r`, which would send the quote line by line. `input_bytes` therefore refuses (returns `None`).
- The question is typed after the paste, so it must not contain a return or escape: whitespace is collapsed to single spaces and control characters dropped.
- `crate::util::basename(path)` gives the file name.

**Step 1: Write the failing tests**

Create `add_to_chat.rs` holding only the tests below and the items they name (empty bodies with `todo!()` are fine), so they compile and fail:

```rust
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
        Card { id: id.into(), provider: "claude".into(), title: format!("{id} title"), cwd: "/p".into(), at: 0, created: 0, status, kind, notice: None }
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
}
```

Add `mod add_to_chat;` to `main.rs`, in alphabetical order (first, before `mod actions;` is fine since `add_to_chat` sorts after `actions`: put it right after `mod actions;`).

**Step 2: Run the tests to verify they fail**

Run: `cargo test -p pocket add_to_chat`
Expected: FAIL (panics at `todo!()` or compile errors for the missing items).

**Step 3: Write the implementation**

Replace everything above the `#[cfg(test)]` module in `add_to_chat.rs` with:

```rust
use crate::status::{Card, Kind, Status};
use crate::util::basename;

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
}

fn lines_text((lo, hi): (usize, usize)) -> String {
    if lo == hi { lo.to_string() } else { format!("{lo}-{hi}") }
}

impl Quote {
    pub fn payload(&self) -> String {
        match &self.body {
            Body::Diff { lines, removed, text } => {
                let side = if *removed { " (before)" } else { "" };
                format!("{}:{}{side}, uncommitted change\n```diff\n{text}\n```", self.path, lines_text(*lines))
            }
        }
    }

    pub fn label(&self) -> String {
        match &self.body {
            Body::Diff { lines, .. } => format!("{}:{} (diff)", basename(&self.path), lines_text(*lines)),
        }
    }

    pub fn icon(&self) -> &'static str {
        match &self.body {
            Body::Diff { .. } => "diff-multiple",
        }
    }

    /// A new session's first prompt: the quote, then the question.
    pub fn prompt(&self, question: &str) -> String {
        match question.trim() {
            "" => self.payload(),
            q => format!("{}\n\n{q}", self.payload()),
        }
    }
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

/// An agent in the "Add to" menu.
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
```

**Step 4: Run the tests to verify they pass**

Run: `cargo test -p pocket add_to_chat`
Expected: PASS (11 tests). Unused-code warnings for `icon`/`prompt` are expected until Task 1.3.

### Task 1.2: A diff pick quotes its lines

**Files:**
- Modify: `packages/desktop/crates/pocket/src/git_ui/diff.rs` (`impl Pick` at 210-287, `impl<I> DiffState<I>` at 378-477, tests at 699-1041)

**Context:**
- `Pick.range` holds diff-line indexes (into `DiffView.lines`), not file line numbers. `ordered(range)` gives the inclusive index range.
- `span(lines, range)` turns it into `(lo, hi, removed)` file line numbers. It returns `None` when the range only covers hunk headers.
- A `Line` has `kind: Kind` (`Hunk | Context | Add | Del`) and `text` without its sign.
- `DiffState.drafting` is the pane whose pick the composer is about. `DiffView::working_file()` is the path only for working-tree diffs (commit diffs can't be picked).
- Test fixture `DIFF` = `"@@ -1,2 +1,3 @@\n a\n-b\n+c\n+d\n"` parses to indexes 0 hunk, 1 ` a`, 2 `-b`, 3 `+c`, 4 `+d`.

**Step 1: Write the failing tests**

In the `tests` module of `diff.rs`, add `use crate::add_to_chat::{Body, Quote};` and these tests:

```rust
    #[test]
    fn a_pick_quotes_its_lines_with_their_signs_numbered_on_the_new_side() {
        let l = parse(DIFF);
        let quote = Pick { range: Some((4, 1)), ..Pick::default() }.quote(&l, "a.rs");
        assert_eq!(quote, Some(Quote { path: "a.rs".into(), body: Body::Diff { lines: (1, 3), removed: false, text: " a\n-b\n+c\n+d".into() } }));
    }

    #[test]
    fn a_pick_of_removed_lines_alone_quotes_their_old_numbers() {
        let l = parse(DIFF);
        let quote = Pick { range: Some((2, 2)), ..Pick::default() }.quote(&l, "a.rs");
        assert_eq!(quote.map(|q| q.body), Some(Body::Diff { lines: (2, 2), removed: true, text: "-b".into() }));
        assert_eq!(Pick { range: Some((0, 0)), ..Pick::default() }.quote(&l, "a.rs"), None);
    }

    #[test]
    fn a_draft_quotes_its_pick_only_while_composing() {
        let mut state = showing(&[(MAIN, "a.rs")]);
        state.panes.get_mut(&MAIN).unwrap().pick = Pick { range: Some((3, 3)), dragging: false, composing: false };
        assert_eq!(state.draft_quote(), None);
        state.panes.get_mut(&MAIN).unwrap().pick.composing = true;
        assert_eq!(state.draft_quote().map(|q| q.path), Some("a.rs".to_string()));
    }
```

**Step 2: Run the tests to verify they fail**

Run: `cargo test -p pocket quote`
Expected: FAIL to compile: no method `quote` on `Pick`, no method `draft_quote`.

**Step 3: Write the implementation**

Add `use crate::add_to_chat::{Body, Quote};` to the imports at the top of `diff.rs`.

Add to `impl Pick` (after `last`):

```rust
    pub fn quote(&self, lines: &[Line], path: &str) -> Option<Quote> {
        let range = ordered(self.range?);
        let (lo, hi, removed) = span(lines, range.clone())?;
        let text = lines[range]
            .iter()
            .filter(|l| l.kind != Kind::Hunk)
            .map(|l| format!("{}{}", match l.kind { Kind::Add => '+', Kind::Del => '-', _ => ' ' }, l.text))
            .collect::<Vec<_>>()
            .join("\n");
        Some(Quote { path: path.to_string(), body: Body::Diff { lines: (lo, hi), removed, text } })
    }
```

Add to `impl<I> DiffState<I>` (after `draft`):

```rust
    pub fn draft_quote(&self) -> Option<Quote> {
        let v = self.draft().filter(|v| v.pick.composing)?;
        v.pick.quote(&v.lines, v.working_file()?)
    }
```

**Step 4: Run the tests to verify they pass**

Run: `cargo test -p pocket quote`
Expected: PASS (the 3 new tests, plus any older test whose name contains "quote").

### Task 1.3: The composer, its agent menu and adding to the input

**Files:**
- Modify: `packages/desktop/crates/pocket/src/add_to_chat.rs`
- Create: `packages/desktop/crates/pocket/src/add_to_chat/composer.rs`
- Create: `packages/desktop/crates/pocket/src/add_to_chat/menu.rs`
- Modify: `packages/desktop/crates/pocket/src/desktop.rs` (field list ~L47-96, `Desktop::new` ~L100-175)

**Context:**
- This task adds the state and views. Nothing renders them until Task 1.4, so the build may warn about unused items. That's fine for now.
- `ChatComposer::new` follows the feature-state pattern: it builds the state and returns its subscriptions; `Desktop::new` only composes (see `DiffState::new` at `diff.rs:366-376`, which this replaces in Task 1.4).
- `TextareaState` is from `gpui_kit::component::input`. Its `InputEvent::PressEnter { secondary: true, .. }` is ⌘↵.
- Existing pieces this task uses:
  - `self.send_input(&terminal_id, &bytes, cx)` (`terminal_view.rs:243`): the one way input reaches a terminal.
  - `self.terminals.sessions.term(&terminal_id).map(|t| t.mode(2004))`: whether that terminal brackets pastes.
  - `self.agents.get(id)` returns the agent `Summary`, which has `terminal_id`.
- Agent list: `crate::sidebar::in_tree(self.cards(project), Some(tree), |c| self.tree_of(c))` keeps the agents of one worktree (see `sidebar.rs:161-163`). `self.cwd()` is the worktree on screen, and `self.repo()` its git state (`.branch`).
- New session: `self.open(Overlay::NewSession, window, cx)`, then `self.reset_new_form(Some(prompt), false, window, cx)` prefills its prompt (see the `prompt` step in `capture.rs:59-60`).
- The UI mirrors `git_ui/diff/composer.rs` (card) and `git_ui/diff/target.rs` (pill + deferred menu). Menu rows follow the prototype's "Add to" menu: provider icon, bold title, sub-line "Claude Code · main". Rows that can't take a quote render at half opacity with no click.

**Step 1: State and Desktop methods**

In `add_to_chat.rs`, add at the top:

```rust
mod composer;
mod menu;

use crate::desktop::Desktop;
use crate::desktop::chrome::Overlay;
use crate::sidebar::in_tree;
use gpui_kit::component::input::{InputEvent, TextareaState};
use gpui_kit::*;
```

(keep the existing `use crate::status…` / `use crate::util::basename;`). Below `default_target`, add:

```rust
pub struct ChatComposer {
    pub(crate) input: Entity<TextareaState>,
    /// The agent picked in the menu; the default applies while it can't take a quote.
    pub(crate) target: Option<String>,
    pub(crate) menu: bool,
}

impl ChatComposer {
    pub fn new(window: &mut Window, cx: &mut Context<Desktop>) -> (Self, Vec<Subscription>) {
        let input = cx.new(|cx| TextareaState::new(window, cx).placeholder("Ask about this selection…").rows(2));
        let subs = vec![cx.subscribe_in(&input, window, |this, _, ev: &InputEvent, window, cx| {
            if let InputEvent::PressEnter { secondary: true, .. } = ev {
                this.add_to_input(window, cx);
            }
        })];
        (Self { input, target: None, menu: false }, subs)
    }
}

impl Desktop {
    fn chat_choices(&self) -> Vec<Choice> {
        let (Some(project), Some(tree)) = (self.project.as_deref(), self.cwd()) else { return Vec::new() };
        let branch = self.repo().map(|r| r.branch.clone()).unwrap_or_default();
        choices(in_tree(self.cards(project), Some(&tree), |c| self.tree_of(c)), &branch)
    }

    fn chat_target(&self, choices: &[Choice]) -> Option<String> {
        default_target(self.chat.target.as_deref(), self.session.as_deref(), choices)
    }

    fn chat_quote(&self) -> Option<Quote> {
        self.diff.draft_quote()
    }

    pub fn add_to_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.agents.observe_only() {
            return;
        }
        let (Some(quote), Some(target)) = (self.chat_quote(), self.chat_target(&self.chat_choices())) else { return };
        let Some(terminal) = self.agents.get(&target).map(|a| a.terminal_id.clone()) else { return };
        let Some(bracketed) = self.terminals.sessions.term(&terminal).map(|t| t.mode(2004)) else { return };
        let question = self.chat.input.read(cx).value();
        match input_bytes(&quote.payload(), &question, bracketed) {
            Some(bytes) => {
                self.send_input(&terminal, &bytes, cx);
                self.cancel_chat(window, cx);
            }
            None => {
                self.error = Some("This agent can't take pasted text right now.".into());
                cx.notify();
            }
        }
    }

    pub fn cancel_chat(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.diff.cancel_draft();
        self.chat.menu = false;
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
```

In `desktop.rs`:
- add `use crate::add_to_chat::ChatComposer;`
- add the field `pub(crate) chat: ChatComposer,` right after `pub(crate) diff: DiffState,`
- in `Desktop::new`, after `let (diff, diff_subs) = DiffState::new(window, cx);` add `let (chat, chat_subs) = ChatComposer::new(window, cx);`
- after `_subs.extend(diff_subs);` add `_subs.extend(chat_subs);`
- add `chat,` after `diff,` in the `Self { … }` literal

**Step 2: The composer card** — create `add_to_chat/composer.rs`:

```rust
use super::Quote;
use crate::desktop::Desktop;
use gpui_kit::component::input::{Escape, Textarea};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;
use ui::Variant;

impl Desktop {
    pub(crate) fn chat_card(&self, quote: &Quote, cx: &mut Context<Self>) -> Div {
        let choices = self.chat_choices();
        let target = self.chat_target(&choices);
        let chosen = target.as_deref().and_then(|id| choices.iter().find(|c| c.id == id));
        let head = div()
            .px(px(16.))
            .pt(px(12.))
            .flex()
            .items_center()
            .justify_between()
            .text_size(px(12.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(icon(quote.icon(), 13., ACCENT))
                    .child(div().font_family(MONO).font_weight(FontWeight::SEMIBOLD).text_color(ACCENT).child(quote.label())),
            )
            .child(div().text_color(TEXT_3).child("esc to dismiss"));
        let field = div().px(px(16.)).py(px(10.)).text_size(px(14.5)).line_height(px(21.75)).child(Textarea::new(&self.chat.input).appearance(false));
        let ready = chosen.is_some();
        let add = ui::button("chat-add", Variant::Accent, None, "Add to input")
            .child(ui::button_kbd("⌘↵"))
            .when(ready, |d| d.on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.add_to_input(window, cx))))
            .when(!ready, |d| d.opacity(0.5).cursor_default());
        let cancel = ui::button("chat-cancel", Variant::Ghost, None, "Cancel").on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.cancel_chat(window, cx)));
        let foot = div()
            .px(px(12.))
            .pt(px(10.))
            .pb(px(12.))
            .flex()
            .items_center()
            .gap(px(8.))
            .border_t_1()
            .border_color(HAIRLINE)
            .child(self.target_picker(chosen, &choices, cx))
            .child(div().flex_1())
            .child(cancel)
            .child(add);
        div()
            .flex()
            .flex_col()
            .rounded(px(16.))
            .bg(SURFACE)
            .shadow(vec![ui::ring(ACCENT_RING, 1.), ui::shadow(rgba(0x1111131a), 8., 24.), ui::shadow(rgba(0x1111130f), 1., 2.)])
            .font_family(SANS)
            .whitespace_normal()
            .on_action(cx.listener(|this, _: &Escape, window, cx| this.cancel_chat(window, cx)))
            .child(head)
            .child(field)
            .child(foot)
    }
}
```

**Step 3: The agent menu** — create `add_to_chat/menu.rs`:

```rust
use super::Choice;
use crate::desktop::Desktop;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;

impl Desktop {
    pub(super) fn target_picker(&self, chosen: Option<&Choice>, choices: &[Choice], cx: &mut Context<Self>) -> Div {
        let pill = div()
            .id("chat-target")
            .h(px(32.))
            .px(px(12.))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(7.))
            .rounded(px(16.))
            .bg(FILL_3)
            .cursor_pointer()
            .hover(|s| s.bg(FILL_4))
            .text_size(px(13.))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.chat.menu = !this.chat.menu;
                cx.notify();
            }));
        let pill = match chosen {
            Some(c) => pill
                .child(provider_icon(&c.provider, 13., TEXT))
                .child(div().font_weight(FontWeight::SEMIBOLD).child(provider_name(&c.provider)))
                .child(div().text_color(TEXT_6).child("·"))
                .child(div().max_w(px(160.)).truncate().text_color(TEXT_2).child(c.title.clone())),
            None => pill.text_color(TEXT_2).child("No agent"),
        };
        let menu = self.chat.menu.then(|| {
            let picked = chosen.map(|c| c.id.clone());
            let items = choices.iter().enumerate().map(|(i, c)| {
                let id = c.id.clone();
                let sub = if c.note.is_empty() { provider_name(&c.provider).to_string() } else { format!("{} · {}", provider_name(&c.provider), c.note) };
                div()
                    .id(("chat-choice", i))
                    .px(px(10.))
                    .py(px(7.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .rounded(px(10.))
                    .text_size(px(13.))
                    .child(provider_icon(&c.provider, 15., TEXT))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(div().truncate().font_weight(FontWeight::SEMIBOLD).child(c.title.clone()))
                            .child(div().truncate().text_size(px(11.5)).text_color(TEXT_3).child(sub)),
                    )
                    .child(div().size(px(14.)).when(picked.as_ref() == Some(&c.id), |d| d.child(icon("check", 14., TEXT))))
                    .when(!c.ready, |d| d.opacity(0.5))
                    .when(c.ready, |d| {
                        d.cursor_pointer().hover(|s| s.bg(FILL_3)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.chat.target = Some(id.clone());
                            this.chat.menu = false;
                            cx.notify();
                        }))
                    })
            });
            let new = div()
                .id("chat-new-session")
                .h(px(34.))
                .px(px(10.))
                .flex()
                .items_center()
                .gap(px(10.))
                .rounded(px(10.))
                .cursor_pointer()
                .hover(|s| s.bg(FILL_3))
                .text_size(px(13.))
                .child(icon("plus", 15., TEXT_2))
                .child("New session…")
                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.chat_in_new_session(window, cx)));
            deferred(
                anchored().anchor(Anchor::BottomLeft).offset(point(px(0.), px(-6.))).snap_to_window_with_margin(px(8.)).child(ui::menu_in(
                    "chat-menu-in",
                    ui::pop(div().id("chat-menu"))
                        .w(px(340.))
                        .p(px(6.))
                        .flex()
                        .flex_col()
                        .child(div().px(px(10.)).pt(px(4.)).pb(px(6.)).text_size(px(11.5)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT_3).child("Add to"))
                        .children(items)
                        .child(div().my(px(4.)).h(px(0.5)).bg(SEPARATOR))
                        .child(new)
                        .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                            this.chat.menu = false;
                            cx.notify();
                        })),
                )),
            )
            .with_priority(1)
        });
        div().relative().child(pill.child(icon("chevron-down", 12., TEXT_3))).children(menu)
    }
}
```

**Step 4: Build**

Run: `cargo build -p pocket && cargo test -p pocket add_to_chat`
Expected: builds (dead-code warnings for `chat_card` etc. are fine until Task 1.4); 11 tests PASS.

### Task 1.4: The diff opens the new composer; comments go

**Files:**
- Modify: `packages/desktop/crates/pocket/src/git_ui/diff.rs`
- Modify: `packages/desktop/crates/pocket/src/git_ui/diff/row.rs` (L116, L144-145)
- Delete: `packages/desktop/crates/pocket/src/git_ui/diff/composer.rs`, `git_ui/diff/target.rs`, `git_ui/diff/comment.rs`, `git_ui/changes/notes.rs`
- Modify: `packages/desktop/crates/pocket/src/git_ui/changes.rs` (L3, L213, L218)
- Modify: `packages/desktop/crates/pocket/src/git_ui/changes/rows.rs` (L120, L170-183)
- Modify: `packages/desktop/crates/pocket/src/desktop.rs` (`Desktop::new`)
- Modify: `packages/desktop/crates/pocket/src/capture.rs` (L51-56, L155)

**Context:** The user chose to replace diff comments with Add to chat. Sent comments, their cards and Resolve, the Changes panel's "Comments" notes, the per-file comment badge and the "N comments" meta item all go. `DiffState` loses its input, target, menu and comments, and with them the generic `I` (it existed only so tests could build it without GPUI). `drafting` defaults to `MAIN` (= 0) through `#[derive(Default)]`.

**Step 1: Update the tests first** (in `diff.rs`'s `tests` module)

- Imports become: `use super::{DiffLoad, DiffState, Pick, Row, changed, highlights, remap, rows};` (keep the other `use` lines and the `add_to_chat` one from Task 1.2).
- Delete these tests and the `sent` helper:
  - `labels_the_pick_by_its_new_lines_in_either_direction`
  - `comments_on_the_pick_by_its_new_lines`
  - `comments_on_removed_lines_alone_by_their_old_lines`
  - `hangs_each_comment_of_the_file_under_its_last_line_on_its_side`
  - `comments_keep_their_line_number_when_the_diff_refreshes`
  - `comments_whose_line_left_the_diff_are_not_shown`
  - `comments_go_to_the_picked_session_while_it_lives`
  - `comments_fall_back_to_the_open_session_then_the_newest`
  - `places_sent_comments_before_the_composer`
  - `labels_ranges_by_their_new_lines`
  - `a_sent_comment_shows_in_every_pane_showing_its_file`
- In `places_the_composer_under_its_line`, drop the last `&[]` argument from all three `rows(…)` calls.
- `showing` becomes:

```rust
    fn showing(panes: &[(PaneId, &str)]) -> DiffState {
        let mut state = DiffState::default();
        for &(pane, path) in panes {
            state.select(pane, path.into(), None);
            state.apply(pane, load(path, DIFF));
        }
        state
    }
```

- Rename `starting_a_comment_in_one_pane_drops_the_pick_in_another` to `asking_in_one_pane_drops_the_pick_in_another` (body unchanged).

Run: `cargo test -p pocket diff`
Expected: FAIL to compile (`DiffState::default`, `rows` arity).

**Step 2: Change `diff.rs`**

1. Module lines 1-4 become just `mod row;`.
2. Remove the imports `use gpui_kit::component::input::{InputEvent, TextareaState};` and `use serde_json::json;`. In `use crate::util::{ago_long, now_ms};` keep both names (still used by the "by" meta item).
3. Delete the `Comment` struct (L24-33).
4. `Row` loses `Comment(usize)`. `rows` becomes:

```rust
/// Lays out the diff, with the composer under the row holding its line.
fn rows(lines: &[Line], split: bool, composer: Option<usize>) -> Vec<Row> {
    let base: Vec<Row> =
        if split { git::split(lines).into_iter().map(|(l, r)| Row::Split(l, r)).collect() } else { (0..lines.len()).map(Row::Unified).collect() };
    let mut out = Vec::with_capacity(base.len() + 1);
    for row in base {
        out.push(row);
        let holds = |i: usize| match row {
            Row::Unified(j) => j == i,
            Row::Split(l, r) => l == Some(i) || r == Some(i),
            Row::Composer => false,
        };
        if composer.is_some_and(holds) {
            out.push(Row::Composer);
        }
    }
    out
}
```

5. Delete `label`, `anchor`, `notes`, `comment_target`, `line_label` (L165-189).
6. In `impl Pick`, delete `label` and `comment` (L279-286).
7. `DiffView::layout` becomes:

```rust
    fn layout(&mut self, reset: bool, split: bool) {
        let rows = rows(&self.lines, split, self.pick.composer_line());
        if reset {
            self.list.reset(rows.len());
        } else {
            let (range, count) = changed(&self.rows, &rows);
            self.list.splice(range, count);
        }
        self.rows = rows;
    }
```

8. `DiffState` becomes the following; delete `impl DiffState { fn new … }`; change `impl<I> DiffState<I>` to `impl DiffState`; delete `with`, `add_comment` and `resolve_comment`; replace every `v.layout(X, self.split, &self.comments)` with `v.layout(X, self.split)`:

```rust
/// The diff each pane shows, and the pane whose pick is being asked about.
#[derive(Default)]
pub struct DiffState {
    pub(crate) panes: HashMap<PaneId, DiffView>,
    pub(crate) split: bool,
    /// The pane whose pick the composer is about; `MAIN` is 0, so it's the default.
    pub(crate) drafting: PaneId,
    pub(crate) viewed: HashSet<String>,
}
```

   The doc comment on `draft_in` becomes `/// Moves the composer to \`pane\`, dropping the pick another pane held.`; on `draft`, `/// The pane the composer is in.`

9. In `diff_view`, delete the comments meta block (the `let comments = …` through its closing `}`, L535-539).
10. In `impl Desktop`, delete `resolve_comment`, `cancel_comment`, `submit_comment`, `comment_target`. Then:
   - In `select_line`, `self.diff.input.update(…)` becomes `self.chat.input.update(…)`.
   - In `end_drag`, `self.diff.input.update(cx, |s, cx| s.focus(window, cx))` becomes `self.chat.input.update(cx, |s, cx| s.focus(window, cx))`.
   - `open_comment` becomes:

```rust
    pub fn open_composer(&mut self, pane: PaneId, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.diff.draft_in(pane);
        let Some(v) = self.diff.panes.get_mut(&pane) else { return };
        if v.pick.open(&v.lines, i) {
            self.chat.input.update(cx, |s, cx| s.set_value("", window, cx));
        }
        self.diff.layout(pane, false);
        self.chat.input.update(cx, |s, cx| s.focus(window, cx));
        cx.notify();
    }
```

   - `select_line`'s doc becomes `/// Picks line \`i\` of \`pane\`'s diff, or with \`extend\` stretches the pick to it.`
   - If `MAIN` is no longer used outside tests, change `use crate::desktop::{Desktop, MAIN};` to `use crate::desktop::Desktop;` (the tests import `MAIN` themselves).
   - Delete the `Pick` doc's "for a comment" → `/// The diff lines picked to ask about: …` (keep the rest of the sentence).

**Step 3: `row.rs`**

- L116: `this.open_comment(pane, i, window, cx)` → `this.open_composer(pane, i, window, cx)`.
- L144-145 become:

```rust
            Row::Composer => div()
                .w_full()
                .children(self.diff.draft_quote().map(|q| {
                    self.chat_card(&q, cx).mt(px(6.)).mb(px(10.)).mr(px(20.)).ml(px(if self.diff.split { NUM + SIGN } else { 2. * NUM + SIGN }))
                }))
                .into_any_element(),
```

**Step 4: Delete the comment views**

```bash
cd /Users/mingo/Developer/self/coding-pocket/packages/desktop/crates/pocket/src
rm git_ui/diff/composer.rs git_ui/diff/target.rs git_ui/diff/comment.rs git_ui/changes/notes.rs
```

- `changes.rs`: delete `mod notes;` (L3), `let notes = self.notes(&repo, cx);` (L213), and the `.when(!notes.is_empty(), |d| d.child(div().id("change-notes")…))` line (L218).
- `changes/rows.rs`: delete `let comments = self.diff.comments.iter().filter(|c| c.path == f.path).count();` (L120) and the whole `.when(comments > 0, |d| { … })` block (L170-183).

**Step 5: `desktop.rs` and `capture.rs`**

- `desktop.rs`: delete `let (diff, diff_subs) = DiffState::new(window, cx);` and `_subs.extend(diff_subs);`. In the `Self { … }` literal, `diff,` becomes `diff: DiffState::default(),`.
- `capture.rs` L51-56 becomes:

```rust
    ("add-to-chat", |d, window, cx| {
        let pane = d.focused_pane();
        if let Some(i) = d.diff.view(pane).and_then(|v| v.lines.iter().position(|l| l.kind == Kind::Add)) {
            d.open_composer(pane, i, window, cx);
        }
    }),
```

- `capture.rs` L155: `d.cancel_comment(window, cx);` → `d.cancel_chat(window, cx);`

**Step 6: Verify**

Run: `cargo build --workspace && cargo test -p pocket`
Expected: builds with no new warnings from `pocket`; all tests PASS.

Run: `cargo clippy --workspace --all-targets 2>&1 | grep -c '^warning'`
Expected: the count recorded before Task 1.1, or lower.

### Task 1.5: Module map and screens

**Files:**
- Modify: `docs/adr/0003-desktop-code-layout.md` (module table, ~L28-39)

**Step 1: Update the ADR**

- In the `git_ui` row, change ``` `diff` view and comments``` to ``` `diff` view```.
- Add this row right after the `git_ui` row:

```markdown
| `add_to_chat` | quoting a selection into an agent's input: the composer, its agent menu, the selection pill |
```

**Step 2: Capture screens**

From the repo root:

```bash
.ui-review/fixture/capture.sh /tmp/add-to-chat-pr1 add-to-chat=session,changes,add-to-chat
```

Expected: `/tmp/add-to-chat-pr1/impl-add-to-chat.png` exists. Open it (Read tool) and check:
- under the first added line, a card with an accent "diff-multiple" icon and a label like `file.rs:12 (diff)`;
- "esc to dismiss" on the right;
- a 2-row question field ("Ask about this selection…");
- a footer with the agent pill (or "No agent" if the fixture has none), Cancel, and "Add to input ⌘↵".

If `capture.sh` reports an unknown step, run it with no screens to list the step names, and use `changes` / `add-to-chat` as named there.

**Step 3: PR gate**

Run: `cargo build --workspace && cargo clippy --workspace --all-targets && cargo test --workspace`
Expected: all green, and the clippy warning count is not above the baseline.

---

## PR 2: Select text in code and markdown to add to chat

**Scope:** Code and markdown quotes; the floating "Add to chat ⌘L" pill after a mouse selection in a file preview; the composer floating by the selection; ⌘L in diff and file panes. Diffs keep their inline composer.
**Depends on:** PR 1
**Done when:** the gate is green; selecting text in a code or markdown preview shows the pill, and clicking it opens the composer, which adds the quote to the agent's input unsent.

### Task 2.1: Code and prose quotes

**Files:**
- Modify: `packages/desktop/crates/pocket/src/add_to_chat.rs`

**Context:**
- Code editor selections are UTF-8 byte ranges into the editor's text (`EditorState::selected_range()`, `value()`).
- A code quote is numbered by the lines the selection touches. A trailing newline (selecting whole lines) doesn't count as touching the next line.
- `crate::syntax::language_for(path)` gives a fence language (`"rust"`, `"typescript"`, …, or `"text"`, which should become a bare fence).
- The markdown view hands back the selection as rendered text, without `**`, `#` and similar markup. `section_of` finds the heading above it by comparing both sides with markup stripped. Lines inside ``` fences never count as headings.

**Step 1: Write the failing tests** (append to the `tests` module; add `use std::ops::Range;` only if a test needs it)

```rust
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
```

**Step 2: Run the tests to verify they fail**

Run: `cargo test -p pocket add_to_chat`
Expected: FAIL to compile (`Quote::code`, `Quote::prose`, `excerpt` missing).

**Step 3: Write the implementation**

Add `use crate::syntax::language_for;` and `use std::ops::Range;` to the imports. `Body` becomes:

```rust
#[derive(Clone, Debug, PartialEq)]
pub enum Body {
    /// `lines` count on the new side, unless the pick only removes lines (`removed`).
    Diff { lines: (usize, usize), removed: bool, text: String },
    Code { lines: (usize, usize), language: &'static str, text: String },
    Prose { section: Option<String>, text: String },
}
```

Replace `impl Quote` with:

```rust
impl Quote {
    /// Quotes the bytes `range` of `text`.
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
```

**Step 4: Run the tests to verify they pass**

Run: `cargo test -p pocket add_to_chat`
Expected: PASS (18 tests).

### Task 2.2: The selection pill and the floating composer

**Files:**
- Create: `packages/desktop/crates/pocket/src/add_to_chat/selection.rs`
- Modify: `packages/desktop/crates/pocket/src/add_to_chat.rs`, `add_to_chat/composer.rs`
- Modify: `packages/desktop/crates/pocket/src/explorer/preview.rs` (module header L1-3, `Views::new` L115-120, `file_view` L393-399)
- Modify: `packages/desktop/crates/pocket/src/git_ui/diff.rs` (`select_line`, `open_composer`)
- Modify: `packages/desktop/crates/pocket/src/git_ui/diff/row.rs` (Composer arm)
- Modify: `packages/desktop/crates/pocket/src/desktop.rs` (render root children, ~L496-500)

**Context:**
- A file pane's state is `self.preview.pane(pane) -> Option<&FilePane>`:
  - `file`: absolute path;
  - `views: Option<Views { code: Entity<EditorState>, md: Entity<TextViewState>, … }>`;
  - `text()`: the file's text.
- `self.preview.body(pane)` returns `explorer::preview::Body::{Markdown, Code, …}`. It's imported as `View` to avoid clashing with our `Body`.
- `relative(path, root)` lives in the private `explorer/preview/header.rs`; re-export it.
- Selection APIs:
  - Code: `EditorState::selected_range() -> Range<usize>` (bytes), `value() -> SharedString`, and `range_to_bounds(&Range<usize>) -> Option<Bounds<Pixels>>` in window coordinates.
  - Markdown: `TextViewState::selected_text() -> String` (empty when nothing is selected).
- GPUI details:
  - `capture_any_mouse_up` runs before the view's own mouse-up, which is where the view settles its selection. So the check is deferred with `cx.defer_in`.
  - `deferred(anchored().position(p))` floats an element at window point `p`, above everything else.
  - `.occlude()` stops clicks reaching what's underneath.
  - `on_mouse_down_out` fires on a press anywhere outside.
- The pill style follows the prototype: a glass pill 36px high, radius 12; "comment" icon, "Add to chat", `⌘L` key cap.
- The floating card is 420px wide and shows the quoted excerpt in a block above the question (code no-wrap and mono, prose wrapped). The diff's inline card doesn't show the excerpt, since the lines are right above it.

**Step 1: State**

In `add_to_chat.rs`:
- add `mod selection;` after `mod menu;`
- add `use workspace::tree::PaneId;`
- extend `ChatComposer`:

```rust
pub struct ChatComposer {
    pub(crate) input: Entity<TextareaState>,
    /// The agent picked in the menu; the default applies while it can't take a quote.
    pub(crate) target: Option<String>,
    pub(crate) menu: bool,
    /// The pane whose file has text selected, and where the "Add to chat" pill shows.
    pub(crate) pill: Option<(PaneId, Point<Pixels>)>,
    /// A file selection being asked about, and where its composer floats.
    pub(crate) file: Option<(Point<Pixels>, Quote)>,
}
```

- `ChatComposer::new` returns `Self { input, target: None, menu: false, pill: None, file: None }`.
- `chat_quote` becomes:

```rust
    fn chat_quote(&self) -> Option<Quote> {
        self.chat.file.as_ref().map(|(_, q)| q.clone()).or_else(|| self.diff.draft_quote())
    }
```

- In `cancel_chat`, after `self.chat.menu = false;` add `(self.chat.file, self.chat.pill) = (None, None);`.

**Step 2: The card shows the excerpt when floating**

In `add_to_chat/composer.rs`:
- `chat_card(&self, quote: &Quote, cx…)` becomes `chat_card(&self, quote: &Quote, quoted: bool, cx: &mut Context<Self>)`
- change `use super::Quote;` to `use super::{Body, Quote};`
- before `let field`, add:

```rust
        let prose = matches!(quote.body, Body::Prose { .. });
        let excerpt = quoted.then(|| {
            div()
                .mx(px(16.))
                .mt(px(10.))
                .px(px(10.))
                .py(px(6.))
                .max_h(px(61.))
                .overflow_hidden()
                .rounded(px(8.))
                .bg(PAGE)
                .border_1()
                .border_color(SEPARATOR)
                .text_color(TEXT_2)
                .map(|d| if prose { d.text_size(px(12.5)).line_height(px(18.)) } else { d.font_family(MONO).text_size(px(11.5)).line_height(px(16.)).whitespace_nowrap() })
                .child(quote.excerpt())
        });
```

- and in the returned card, `.child(head).child(field)` becomes `.child(head).children(excerpt).child(field)`.

In `git_ui/diff/row.rs`, the Composer arm's `self.chat_card(&q, cx)` becomes `self.chat_card(&q, false, cx)`.

**Step 3: `add_to_chat/selection.rs`**

```rust
use super::Quote;
use crate::desktop::Desktop;
use crate::explorer::preview::{Body as View, relative};
use gpui_kit::*;
use theme::*;
use workspace::tree::PaneId;

/// Offers the pill where a mouse selection in `pane`'s file ends.
pub(crate) fn offers_chat(pane: PaneId, body: Div, cx: &Context<Desktop>) -> Div {
    body.capture_any_mouse_up(cx.listener(move |_, ev: &MouseUpEvent, window, cx| {
        if ev.button != MouseButton::Left {
            return;
        }
        let at = ev.position;
        // The view settles its selection in its own mouse-up, which runs after this capture.
        cx.defer_in(window, move |this, _, cx| this.offer_chat(pane, at, cx));
    }))
}

impl Desktop {
    fn offer_chat(&mut self, pane: PaneId, at: Point<Pixels>, cx: &mut Context<Self>) {
        let pill = self.has_selection(pane, cx).then_some((pane, at));
        if pill != self.chat.pill {
            self.chat.pill = pill;
            cx.notify();
        }
    }

    fn has_selection(&self, pane: PaneId, cx: &App) -> bool {
        let Some(views) = self.preview.pane(pane).and_then(|f| f.views.as_ref()) else { return false };
        match self.preview.body(pane) {
            View::Code => !views.code.read(cx).selected_range().is_empty(),
            View::Markdown => !views.md.read(cx).selected_text().trim().is_empty(),
            _ => false,
        }
    }

    /// What's selected in `pane`'s file, and where on screen it ends when the view can tell.
    fn selection_quote(&self, pane: PaneId, cx: &App) -> Option<(Quote, Option<Point<Pixels>>)> {
        let f = self.preview.pane(pane)?;
        let (file, views) = (f.file.as_deref()?, f.views.as_ref()?);
        let path = self.cwd().map_or_else(|| file.to_string(), |root| relative(file, &root));
        match self.preview.body(pane) {
            View::Code => {
                let code = views.code.read(cx);
                let range = code.selected_range();
                let near = code.range_to_bounds(&range).map(|b| b.bottom_left());
                Some((Quote::code(&path, &code.value(), range)?, near))
            }
            View::Markdown => Some((Quote::prose(&path, f.text()?, &views.md.read(cx).selected_text())?, None)),
            _ => None,
        }
    }

    pub(crate) fn ask_about_text(&mut self, pane: PaneId, window: &mut Window, cx: &mut Context<Self>) {
        let Some((quote, near)) = self.selection_quote(pane, cx) else { return };
        let at = self.chat.pill.take().map(|(_, at)| at).or(near).unwrap_or_else(|| window.mouse_position());
        self.diff.cancel_draft();
        (self.chat.file, self.chat.menu) = (Some((at, quote)), false);
        self.chat.input.update(cx, |s, cx| {
            s.set_value("", window, cx);
            s.focus(window, cx);
        });
        cx.notify();
    }

    pub(crate) fn chat_pill(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let (pane, at) = self.chat.pill?;
        let pill = div()
            .id("chat-pill")
            .h(px(36.))
            .px(px(12.))
            .flex()
            .items_center()
            .gap(px(8.))
            .rounded(px(12.))
            .cursor_pointer()
            .text_size(px(13.))
            .font_weight(FontWeight::MEDIUM)
            .text_color(TEXT)
            .child(icon("comment", 14., TEXT_2))
            .child("Add to chat")
            .child(ui::kbd("⌘L"))
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.ask_about_text(pane, window, cx)))
            .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                this.chat.pill = None;
                cx.notify();
            }));
        Some(deferred(anchored().position(at + point(px(0.), px(8.))).snap_to_window_with_margin(px(8.)).child(ui::glass(pill))).with_priority(2))
    }

    pub(crate) fn chat_popover(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let (at, quote) = self.chat.file.as_ref()?;
        let card = self.chat_card(quote, true, cx).w(px(420.)).occlude();
        Some(deferred(anchored().position(*at + point(px(0.), px(8.))).snap_to_window_with_margin(px(8.)).child(card)).with_priority(2))
    }
}
```

In `add_to_chat.rs` add `pub(crate) use selection::offers_chat;` below the `mod` lines.

**Step 4: Wire it in**

- `explorer/preview.rs`: after `mod markdown;` add `pub(crate) use header::relative;`.
- `explorer/preview.rs` `file_view`: the two arms become

```rust
            Body::Markdown => offers_chat(pane, markdown_pane(views, cx), cx).into_any_element(),
            Body::Code => offers_chat(pane, code_pane(&views.code, marks.clone()), cx).into_any_element(),
```

  with `use crate::add_to_chat::offers_chat;` added to its imports.

- `explorer/preview.rs` `Views::new`: the `_edits` subscription body becomes

```rust
            if let InputEvent::Change = ev {
                let text = code.read(cx).value();
                this.edited(pane, text, cx);
                if this.chat.pill.take().is_some() {
                    cx.notify();
                }
            }
```

- `git_ui/diff.rs`: in `select_line` and `open_composer`, add `self.chat.file = None;` as the first line.
- `desktop.rs` render: after `.children(overlay)` add `.children(self.chat_pill(cx)).children(self.chat_popover(cx))`.

**Step 5: Build and test**

Run: `cargo build -p pocket && cargo test -p pocket`
Expected: builds without new warnings; all tests PASS.

Try it by hand: `cargo run --release -p pocket`.
1. Open a `.rs` file from Explorer and drag-select a few lines. The pill appears under the mouse.
2. Click the pill. The card floats there, showing `a.rs:L-H`, the excerpt, the question field and the agent pill.
3. ⌘↵ pastes into the agent's input without sending, stays put, and shows the "Added to …'s input" toast.
4. Repeat in a markdown file. The label reads `FILE.md › Section`.

### Task 2.3: ⌘L, and screens

**Files:**
- Modify: `packages/desktop/crates/pocket/src/actions.rs` (L5 `actions!` list, `bindings()`)
- Modify: `packages/desktop/crates/pocket/src/add_to_chat.rs`
- Modify: `packages/desktop/crates/pocket/src/desktop.rs` (root `.on_action` list ~L470-497)
- Modify: `packages/desktop/crates/pocket/src/capture.rs` (STEPS)

**Context:**
- `self.focused_pane()` is the focused pane; `self.pane_tab(pane) -> Option<Tab>`.
- The tab types are `workspace::Tab::Doc(workspace::Doc::Diff(path) | Doc::File(path) | …)`.
- In a diff, ⌘L opens the composer on the current pick, working-tree diffs only.
- In a file, ⌘L asks about the selection; with no pill the card floats at the selection's end (code) or the mouse (markdown).
- `cmd-l` is already bound to `FocusAddress` in the browser context only. A context-free binding coexists, and the browser's more specific one wins there.

**Step 1: The action**

- `actions.rs`: add `AddToChat` to the `actions!(desktop, [...])` list (after `EqualizePanes`), and `KeyBinding::new("cmd-l", AddToChat, None),` to the `vec![` in `bindings()` after the `cmd-k` line.
- `add_to_chat.rs` — add `use workspace::{Doc, Tab};` and `use crate::actions::AddToChat;`, and to `impl Desktop`:

```rust
    pub(crate) fn add_to_chat(&mut self, _: &AddToChat, window: &mut Window, cx: &mut Context<Self>) {
        let pane = self.focused_pane();
        match self.pane_tab(pane) {
            Some(Tab::Doc(Doc::Diff(_))) => {
                if let Some(i) = self.diff.view(pane).filter(|v| v.at.is_none()).and_then(|v| v.pick.last()) {
                    self.open_composer(pane, i, window, cx);
                }
            }
            Some(Tab::Doc(Doc::File(_))) => self.ask_about_text(pane, window, cx),
            _ => {}
        }
    }
```

- `desktop.rs`: add `.on_action(cx.listener(Self::add_to_chat))` to the root's `.on_action` list (after `open_selected`).

**Step 2: Capture steps**

In `capture.rs`:
- add `use crate::add_to_chat::Quote;`
- bump `STEPS: [(&str, Step); 24]` to `26`
- after the `"file"` step add:

```rust
    ("pill", |d, _, _| d.chat.pill = Some((d.focused_pane(), point(px(420.), px(260.))))),
    ("ask-file", |d, _, _| {
        let pane = d.focused_pane();
        let Some((path, text)) = d.preview.pane(pane).and_then(|f| Some((f.file.clone()?, f.text()?.to_string()))) else { return };
        let end = text.match_indices('\n').nth(2).map_or(text.len(), |(i, _)| i);
        d.chat.file = Quote::code(&crate::util::basename(&path), &text, 0..end).map(|q| (point(px(420.), px(260.)), q));
    }),
```

- in `reset`, `cancel_chat` already clears `pill` and `file`.

**Step 3: Build, test, capture**

Run: `cargo build -p pocket && cargo test -p pocket`
Expected: green.

From the repo root:

```bash
.ui-review/fixture/capture.sh /tmp/add-to-chat-pr2 pill=session,explore,file,pill ask-file=session,explore,file,ask-file add-to-chat=session,changes,add-to-chat
```

Expected: three PNGs. Check with the Read tool:
- `impl-pill.png`: a glass pill "Add to chat ⌘L" over the file.
- `impl-ask-file.png`: a 420px card with `name:1-3`, a mono excerpt of 3 lines, the question field and the footer.
- `impl-add-to-chat.png`: the same as PR1, with no excerpt block.

Compare against `.ui-review/prototypes/add-to-chat.html` (open it with the agent-browser skill if needed).

**Step 4: PR gate**

Run: `cargo build --workspace && cargo clippy --workspace --all-targets && cargo test --workspace`
Expected: all green; the clippy warning count is not above the baseline.
