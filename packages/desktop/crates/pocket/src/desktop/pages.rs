use super::Desktop;
use super::chrome::Screen;
use crate::actions::{OpenActivity, OpenAutomations, OpenBoard, OpenSettings};
use agents::Agents;
use gpui_kit::*;

/// A page the sidebar, the rail and the palette all go to.
pub(crate) struct Page {
    pub screen: Screen,
    pub name: &'static str,
    pub icon: &'static str,
    /// The shortcut the palette shows beside it.
    pub keys: Option<&'static str>,
    offered: fn(&Agents) -> bool,
}

/// In the order the sidebar, the rail and the palette list them.
const PAGES: [Page; 3] = [
    Page { screen: Screen::Activity, name: "Activity", icon: "activity", keys: None, offered: |_| true },
    Page { screen: Screen::Board, name: "Board", icon: "blocks", keys: None, offered: Agents::board_offered },
    Page { screen: Screen::Automations, name: "Automations", icon: "bolt", keys: Some("⌘ ⇧ A"), offered: Agents::automations_offered },
];

/// The pages pocketd runs what they need for.
pub(crate) fn offered(agents: &Agents) -> impl Iterator<Item = &'static Page> + '_ {
    PAGES.iter().filter(|p| (p.offered)(agents))
}

impl Desktop {
    /// Goes to `screen`, as its nav row, rail button or palette entry does.
    pub(crate) fn go_to_screen(&mut self, screen: Screen, window: &mut Window, cx: &mut Context<Self>) {
        match screen {
            Screen::Activity => self.open_activity(&OpenActivity, window, cx),
            Screen::Board => self.open_board(&OpenBoard, window, cx),
            Screen::Automations => self.open_automations(&OpenAutomations, window, cx),
            Screen::Inbox => self.open_inbox(window, cx),
            Screen::Settings => self.open_settings(&OpenSettings, window, cx),
            Screen::Sessions => {
                self.screen = Screen::Sessions;
                cx.notify();
            }
        }
    }

    /// A rail button: goes to its page, or back to the sessions from it.
    pub(crate) fn toggle_screen(&mut self, screen: Screen, window: &mut Window, cx: &mut Context<Self>) {
        if self.screen != screen {
            return self.go_to_screen(screen, window, cx);
        }
        match screen {
            Screen::Board => self.leave_board(window, cx),
            Screen::Activity => self.leave_activity(cx),
            Screen::Sessions | Screen::Inbox | Screen::Automations | Screen::Settings => self.go_to_screen(Screen::Sessions, window, cx),
        }
    }

    /// Esc on the page on screen; false when the page has no use for it.
    pub(crate) fn escape_screen(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        match self.screen {
            Screen::Settings => self.escape_settings(window, cx),
            Screen::Activity => self.escape_activity(window, cx),
            Screen::Automations => self.escape_automations(cx),
            Screen::Board => self.escape_board(window, cx),
            Screen::Sessions | Screen::Inbox => return false,
        }
        true
    }

    /// The keys the page on screen takes; false when it leaves `key` alone.
    pub(crate) fn screen_key(&mut self, key: &Keystroke, window: &mut Window, cx: &mut Context<Self>) -> bool {
        match self.screen {
            Screen::Board => self.board_key(key, window, cx),
            Screen::Activity => self.activity_key(key, window, cx),
            Screen::Sessions | Screen::Inbox | Screen::Automations | Screen::Settings => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::offered;
    use crate::desktop::chrome::Screen;
    use agents::{Agents, Event};

    fn screens(agents: &Agents) -> Vec<Screen> {
        offered(agents).map(|p| p.screen).collect()
    }

    #[test]
    fn board_and_automations_show_only_once_pocketd_runs_them() {
        assert_eq!(screens(&Agents::default()), [Screen::Activity]);
        let mut a = Agents::default();
        a.apply(Event::Connected { scopes: vec![], caps: vec!["board.v1".into(), "automations.v1".into()], version: String::new() });
        assert_eq!(screens(&a), [Screen::Activity, Screen::Board, Screen::Automations]);
    }
}
