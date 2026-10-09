mod composer;

use crate::desktop::Desktop;
use crate::terminals::link;
use agents::CreateReply;
use gpui_kit::component::input::{InputEvent, TextareaState};
use gpui_kit::*;
use serde_json::json;
use store::LaunchPick;
use workspace::tree::PaneId;

/// The prompt a focused pane with no tabs offers, which starts an agent in that pane.
pub(crate) struct EmptyPane {
    pub(crate) prompt: Entity<TextareaState>,
    pub(crate) launch: Launch,
    /// The agent picked here for a Project; like the new session sheet's, it is remembered once a session starts.
    chosen: Option<(String, LaunchPick)>,
    menu: Option<Menu>,
    pub(crate) drawn: Drawn,
}

/// The composer chip whose menu is open.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Menu {
    Agent,
    Model,
    Access,
}

/// Whether the focused pane showed the prompt in the frame last drawn, and in the one before, so the prompt takes the keys only as it appears.
#[derive(Default)]
pub(crate) struct Drawn {
    pub(crate) last: bool,
    before: bool,
}

impl Drawn {
    /// Called as each frame starts; the frame sets `last` again if the prompt is still drawn.
    pub(crate) fn begin_frame(&mut self) {
        self.before = std::mem::take(&mut self.last);
    }

    /// Marks the prompt drawn this frame; true if it just appeared.
    fn draw(&mut self) -> bool {
        self.last = true;
        !self.before
    }
}

/// The create this prompt sent, and where from: its worktree, the pane its terminal opens in, and the words to give back if it fails.
#[derive(Default)]
pub(crate) struct Launch {
    pub(crate) reply: CreateReply,
    worktree: String,
    pub(crate) pane: PaneId,
    prompt: String,
}

/// What a failed create of this prompt's changes on screen.
#[derive(Debug, PartialEq)]
struct Failed {
    /// The words sent, unless new ones were typed since.
    words: Option<String>,
    /// A failure the prompt can't show, being off screen or in another worktree, goes to the page.
    on_page: bool,
}

impl Launch {
    /// One create at a time, and only with something to ask.
    fn ready(&self, prompt: &str) -> bool {
        !self.reply.waiting() && !prompt.trim().is_empty()
    }

    fn sent(&mut self, request: String, worktree: String, pane: PaneId, prompt: String) {
        self.reply.sent(request);
        (self.worktree, self.pane, self.prompt) = (worktree, pane, prompt);
    }

    fn starting_in(&self, worktree: &str) -> bool {
        self.reply.waiting() && self.worktree == worktree
    }

    fn error_in(&self, worktree: &str) -> Option<&(String, String)> {
        self.reply.error.as_ref().filter(|_| self.worktree == worktree)
    }

    /// Answers `request` if it was this prompt's.
    fn failed(&mut self, request: &str, error: (String, String), typed: &str, shown: bool, cwd: Option<&str>) -> Option<Failed> {
        if !self.reply.answer(request, Some(error)) {
            return None;
        }
        Some(Failed { words: typed.is_empty().then(|| self.prompt.clone()), on_page: !shown || cwd != Some(self.worktree.as_str()) })
    }
}

/// A pane with no tabs offers the prompt only with a Project and an open worktree, in a window allowed to spawn.
fn offers_prompt(project: Option<&str>, tree: Option<&str>, observe_only: bool) -> bool {
    project.is_some() && tree.is_some() && !observe_only
}

/// The agent picked here, if it was picked for this Project.
fn chosen_for<'a>(chosen: &'a Option<(String, LaunchPick)>, project: Option<&str>) -> Option<&'a LaunchPick> {
    chosen.as_ref().filter(|(p, _)| Some(p.as_str()) == project).map(|(_, pick)| pick)
}

impl EmptyPane {
    pub fn new(window: &mut Window, cx: &mut Context<Desktop>) -> (Self, Vec<Subscription>) {
        let prompt = cx.new(|cx| TextareaState::new(window, cx).placeholder("Describe a task or paste an error").auto_grow(2, 8));
        let subs = vec![cx.subscribe_in(&prompt, window, |this, _, ev: &InputEvent, window, cx| match ev {
            InputEvent::PressEnter { secondary: true, .. } => this.start_from_empty_pane(window, cx),
            InputEvent::Change => cx.notify(),
            _ => {}
        })];
        (Self { prompt, launch: Launch::default(), chosen: None, menu: None, drawn: Drawn::default() }, subs)
    }
}

impl Desktop {
    pub(crate) fn offers_empty_prompt(&self) -> bool {
        offers_prompt(self.project.as_deref(), self.cwd().as_deref(), self.agents.observe_only())
    }

    pub(crate) fn focus_empty_prompt(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.empty_pane.prompt.update(cx, |s, cx| s.focus(window, cx));
    }

    fn launch_pick(&self) -> LaunchPick {
        chosen_for(&self.empty_pane.chosen, self.project.as_deref()).map_or_else(|| self.last_pick(), LaunchPick::clone)
    }

    pub(crate) fn toggle_empty_menu(&mut self, menu: Menu, cx: &mut Context<Self>) {
        self.empty_pane.menu = (self.empty_pane.menu != Some(menu)).then_some(menu);
        cx.notify();
    }

    fn close_empty_menu(&mut self, cx: &mut Context<Self>) {
        self.empty_pane.menu = None;
        cx.notify();
    }

    /// Changes the pick for this Project and closes the menu it came from.
    fn pick_empty(&mut self, change: impl FnOnce(LaunchPick) -> LaunchPick, cx: &mut Context<Self>) {
        if let Some(project) = self.project.clone() {
            self.empty_pane.chosen = Some((project, change(self.launch_pick())));
        }
        self.close_empty_menu(cx);
    }

    /// The prompt clears at once; the terminal opens in this pane once pocketd lists it (`Terminals::arrived`).
    fn start_from_empty_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let prompt = self.empty_pane.prompt.read(cx).value().trim().to_string();
        let (Some(project), Some(tree)) = (self.project.clone(), self.cwd()) else { return };
        if !self.offers_empty_prompt() || !self.empty_pane.launch.ready(&prompt) {
            return;
        }
        if self.terminals.link.stale().is_some() {
            self.error = Some(link::UPDATING.into());
            return cx.notify();
        }
        let pick = self.launch_pick();
        let mut spec = pick.spec(&project, json!({"worktree": tree}), &prompt);
        self.store.agents.launch(&mut spec);
        let request = self.outbox.create(spec);
        self.store.repos.entry(project).or_default().launch = pick;
        self.save_soon(cx);
        let pane = self.workspace(&tree).tree.focused;
        self.empty_pane.launch.sent(request, tree, pane, prompt);
        (self.empty_pane.chosen, self.empty_pane.menu) = (None, None);
        self.empty_pane.prompt.update(cx, |s, cx| s.set_value("", window, cx));
        cx.notify();
    }

    /// Whether the failed create was this prompt's.
    pub(crate) fn empty_prompt_failed(&mut self, request: &str, message: &str, detail: &str, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let typed = self.empty_pane.prompt.read(cx).value();
        let cwd = self.cwd();
        let Some(failed) = self.empty_pane.launch.failed(request, (message.into(), detail.into()), &typed, self.empty_pane.drawn.last, cwd.as_deref()) else {
            return false;
        };
        if let Some(words) = failed.words {
            self.empty_pane.prompt.update(cx, |s, cx| s.set_value(words, window, cx));
        }
        if failed.on_page {
            self.error = Some(message.into());
        }
        true
    }

    fn fill_empty_prompt(&mut self, text: SharedString, window: &mut Window, cx: &mut Context<Self>) {
        self.empty_pane.prompt.update(cx, |s, cx| {
            s.set_value(text, window, cx);
            s.focus(window, cx);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::{Drawn, Failed, Launch, chosen_for, offers_prompt};
    use store::LaunchPick;

    fn failure() -> (String, String) {
        ("No such project".into(), String::new())
    }

    #[test]
    fn the_prompt_needs_a_project_an_open_worktree_and_the_right_to_spawn() {
        assert!(offers_prompt(Some("/repo"), Some("/repo"), false));
        assert!(!offers_prompt(None, Some("/repo"), false));
        assert!(!offers_prompt(Some("/repo"), None, false));
        assert!(!offers_prompt(Some("/repo"), Some("/repo"), true));
    }

    #[test]
    fn the_prompt_takes_the_keys_only_on_the_frame_it_appears() {
        let mut drawn = Drawn::default();
        drawn.begin_frame();
        assert!(drawn.draw());
        drawn.begin_frame();
        assert!(!drawn.draw());
        drawn.begin_frame();
        drawn.begin_frame();
        assert!(drawn.draw());
    }

    #[test]
    fn an_agent_picked_here_holds_only_for_the_project_it_was_picked_in() {
        let chosen = Some(("/repo".to_string(), LaunchPick { provider: "codex".into(), ..LaunchPick::default() }));
        assert_eq!(chosen_for(&chosen, Some("/repo")).map(|p| p.provider.as_str()), Some("codex"));
        assert!(chosen_for(&chosen, Some("/other")).is_none());
        assert!(chosen_for(&chosen, None).is_none());
    }

    #[test]
    fn a_prompt_starts_once_and_only_with_something_to_ask() {
        let mut launch = Launch::default();
        assert!(!launch.ready("  \n"));
        assert!(launch.ready("Fix the failing tests"));
        launch.sent("r1".into(), "/repo".into(), 1, "Fix the failing tests".into());
        assert!(!launch.ready("Fix the failing tests"));
    }

    #[test]
    fn a_prompt_shows_it_is_starting_only_in_its_worktree_until_answered() {
        let mut launch = Launch::default();
        launch.sent("r1".into(), "/repo".into(), 1, "Fix it".into());
        assert!(launch.starting_in("/repo"));
        assert!(!launch.starting_in("/repo/.worktrees/fix"));
        launch.reply.answer("r1", None);
        assert!(!launch.starting_in("/repo"));
    }

    #[test]
    fn only_its_own_request_is_answered_and_a_failure_stays_until_the_next_send() {
        let mut launch = Launch::default();
        launch.sent("r1".into(), "/repo".into(), 1, "Fix it".into());
        assert_eq!(launch.failed("r2", failure(), "", true, Some("/repo")), None);
        assert!(launch.failed("r1", failure(), "", true, Some("/repo")).is_some());
        assert!(launch.error_in("/repo").is_some() && launch.ready("again"));
        launch.sent("r3".into(), "/repo".into(), 1, "again".into());
        assert!(launch.error_in("/repo").is_none());
    }

    #[test]
    fn a_failure_shows_on_the_prompt_only_while_its_worktree_shows_it() {
        let fail = |shown, cwd| {
            let mut launch = Launch::default();
            launch.sent("r1".into(), "/repo".into(), 1, "Fix it".into());
            let failed = launch.failed("r1", failure(), "", shown, cwd).unwrap();
            (failed.on_page, launch.error_in("/repo/.worktrees/fix").is_some())
        };
        assert_eq!(fail(true, Some("/repo")), (false, false));
        assert_eq!(fail(true, Some("/repo/.worktrees/fix")), (true, false));
        assert_eq!(fail(false, Some("/repo")), (true, false));
    }

    #[test]
    fn a_failure_gives_its_words_back_only_if_nothing_new_was_typed() {
        let mut launch = Launch::default();
        launch.sent("r1".into(), "/repo".into(), 1, "Fix it".into());
        assert_eq!(launch.failed("r1", failure(), "", true, Some("/repo")), Some(Failed { words: Some("Fix it".into()), on_page: false }));
        launch.sent("r2".into(), "/repo".into(), 1, "Fix it".into());
        assert_eq!(launch.failed("r2", failure(), "Something else", true, Some("/repo")).and_then(|f| f.words), None);
    }
}
