use crate::actions::{GoToUpNext, JumpTo, NextNeedsYou, NextSession, PrevSession};
use crate::desktop::Desktop;
use crate::desktop::chrome::Side;
use crate::sidebar::in_tree;
use crate::sidebar::sessions::matching;
use crate::status::{self, Card};
use gpui_kit::*;
use std::time::Duration;

pub const CHIP_DELAY_MS: u64 = 280;

/// Whether ⌘ alone is down with nothing over the window, which starts the chip timer.
pub fn arms_chips(m: &Modifiers, overlay_open: bool) -> bool {
    *m == Modifiers::command() && !overlay_open
}

/// The sessions ⌘n and ⌃Tab walk, in the order the Sessions column shows them: pinned first.
pub(crate) fn walkable(cards: Vec<Card>, tree: Option<&str>, tree_of: impl Fn(&str) -> Option<String>, query: &str) -> Vec<Card> {
    let mut cards = matching(in_tree(cards, tree, tree_of), query);
    cards.sort_by_key(|c| !c.pinned);
    cards
}

/// The ⌘1–⌘9 chips on session rows, shown once ⌘ has been held alone for `CHIP_DELAY_MS`.
#[derive(Default)]
pub struct Chips {
    pub(crate) shown: bool,
    timer: Option<Task<()>>,
}

impl Chips {
    pub fn new(window: &mut Window, cx: &mut Context<Desktop>) -> (Self, Vec<Subscription>) {
        let subs = vec![cx.observe_window_activation(window, |this, window, cx| {
            if !window.is_window_active() && this.chips.hide() {
                cx.notify();
            }
        })];
        (Self::default(), subs)
    }

    /// Returns whether the chips were showing.
    fn hide(&mut self) -> bool {
        self.timer = None;
        std::mem::take(&mut self.shown)
    }
}

impl Desktop {
    pub(crate) fn visible_sessions(&self, cx: &App) -> Vec<Card> {
        let Some(project) = self.project.as_deref() else { return Vec::new() };
        let tree = self.cwd();
        let query = self.sidebar.search.read(cx).value();
        walkable(self.cards(project), tree.as_deref(), |cwd| self.tree_of(cwd), &query)
    }

    fn all_cards(&self) -> Vec<Card> {
        self.projects().iter().flat_map(|p| self.cards(p)).collect()
    }

    fn go_to(&mut self, id: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = id else { return };
        self.overlay = None;
        self.side = Side::Sessions;
        self.focus_agent(&id, window, cx);
    }

    pub(crate) fn next_needs_you(&mut self, _: &NextNeedsYou, window: &mut Window, cx: &mut Context<Self>) {
        let next = status::next_needs_you(&self.all_cards(), self.session());
        self.go_to(next, window, cx);
    }

    pub(crate) fn go_to_up_next(&mut self, _: &GoToUpNext, window: &mut Window, cx: &mut Context<Self>) {
        let next = status::next_up(&self.all_cards(), self.session());
        self.go_to(next, window, cx);
    }

    pub(crate) fn jump_to(&mut self, a: &JumpTo, window: &mut Window, cx: &mut Context<Self>) {
        if self.overlay.is_some() {
            return;
        }
        let nth = self.visible_sessions(cx).into_iter().nth(a.0.saturating_sub(1)).map(|c| c.id);
        self.go_to(nth, window, cx);
    }

    fn step_session(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        let ids: Vec<String> = self.visible_sessions(cx).into_iter().map(|c| c.id).collect();
        let next = status::cycle(&ids, self.session(), forward);
        self.go_to(next, window, cx);
    }

    pub(crate) fn next_session(&mut self, _: &NextSession, window: &mut Window, cx: &mut Context<Self>) {
        self.step_session(true, window, cx);
    }

    pub(crate) fn prev_session(&mut self, _: &PrevSession, window: &mut Window, cx: &mut Context<Self>) {
        self.step_session(false, window, cx);
    }

    pub(crate) fn on_modifiers(&mut self, ev: &ModifiersChangedEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.chips.hide() {
            cx.notify();
        }
        if !arms_chips(&ev.modifiers, self.overlay.is_some()) {
            return;
        }
        self.chips.timer = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(CHIP_DELAY_MS)).await;
            this.update(cx, |d, cx| {
                d.chips.shown = true;
                cx.notify();
            })
            .ok();
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::{arms_chips, walkable};
    use crate::status::{self, Card};
    use agents::Summary;
    use gpui_kit::Modifiers;

    fn card(id: &str, status: &str, cwd: &str) -> Card {
        status::card(&Summary { id: id.into(), title: format!("Fix {id}"), status: status.into(), attached: true, ..Default::default() }, cwd)
    }

    fn ids(query: &str) -> Vec<String> {
        let cards = vec![card("a", "idle", "/app"), card("b", "working", "/app-wt"), card("c", "done", "/app"), card("d", "idle", "/app")];
        walkable(cards, Some("/app"), |cwd| Some(cwd.to_string()), query).into_iter().map(|c| c.id).collect()
    }

    #[test]
    fn cmd_n_counts_the_filtered_list() {
        assert_eq!(ids(""), vec!["a", "c", "d"]);
        assert_eq!(ids("fix c"), vec!["c"]);
    }

    #[test]
    fn pinned_sessions_come_first_in_their_own_order() {
        let pinned = |id: &str| Card { pinned: true, ..card(id, "idle", "/app") };
        let cards = vec![card("a", "idle", "/app"), pinned("b"), card("c", "idle", "/app"), pinned("d")];
        let order: Vec<String> = walkable(cards, Some("/app"), |cwd| Some(cwd.to_string()), "").into_iter().map(|c| c.id).collect();
        assert_eq!(order, vec!["b", "d", "a", "c"]);
    }

    #[test]
    fn chips_arm_on_cmd_alone_and_never_over_an_overlay() {
        let cmd_shift = Modifiers { shift: true, ..Modifiers::command() };
        assert!(arms_chips(&Modifiers::command(), false));
        assert!(!arms_chips(&Modifiers::command(), true));
        assert!(!arms_chips(&cmd_shift, false));
        assert!(!arms_chips(&Modifiers::default(), false));
    }
}
