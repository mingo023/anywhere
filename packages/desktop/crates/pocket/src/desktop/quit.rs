use crate::actions::Quit;
use crate::desktop::Desktop;
use crate::desktop::chrome::{Confirm, Overlay};
use crate::status::Status;
use gpui_kit::*;
use std::sync::atomic::{AtomicBool, Ordering};

/// Set while the quit alert is up: ⌘Q still reaches the window under the sheet, and a second alert would queue behind the first.
static ALERT_UP: AtomicBool = AtomicBool::new(false);
const QUIT: usize = 0;

#[derive(Debug, PartialEq)]
enum Ask {
    DiscardEdits(usize),
    Alert,
    Nothing,
    Quit,
}

fn ask(unsaved: usize, alert_up: bool, confirm: bool) -> Ask {
    if unsaved > 0 {
        Ask::DiscardEdits(unsaved)
    } else if alert_up {
        Ask::Nothing
    } else if confirm {
        Ask::Alert
    } else {
        Ask::Quit
    }
}

impl Desktop {
    pub fn quit(&mut self, _: &Quit, window: &mut Window, cx: &mut Context<Self>) {
        let working = self.agents.list.iter().filter(|a| Status::of(a) == Some(Status::Working)).count();
        match ask(self.preview.drafts.len(), ALERT_UP.load(Ordering::Relaxed), self.store.general.confirm_quit.asks(working)) {
            Ask::DiscardEdits(unsaved) => {
                self.confirm = Some(Confirm::Quit(unsaved));
                self.overlay = Some(Overlay::Confirm);
                cx.notify();
            }
            Ask::Alert => {
                ALERT_UP.store(true, Ordering::Relaxed);
                let answer = window.prompt(PromptLevel::Info, "Are you sure you want to quit?", None, &[PromptButton::ok("Quit"), PromptButton::cancel("Cancel")], cx);
                cx.spawn(async move |_, cx| {
                    let quit = answer.await == Ok(QUIT);
                    ALERT_UP.store(false, Ordering::Relaxed);
                    if quit {
                        cx.update(|cx| cx.quit());
                    }
                })
                .detach();
            }
            Ask::Nothing => {}
            Ask::Quit => cx.quit(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Ask, ask};

    #[test]
    fn quitting_asks_about_unsaved_edits_before_the_alert_and_never_stacks_alerts() {
        assert_eq!(ask(2, false, true), Ask::DiscardEdits(2));
        assert_eq!(ask(0, false, true), Ask::Alert);
        assert_eq!(ask(0, true, true), Ask::Nothing);
    }

    #[test]
    fn with_nothing_to_confirm_quitting_quits_unless_edits_are_unsaved() {
        assert_eq!(ask(0, false, false), Ask::Quit);
        assert_eq!(ask(1, false, false), Ask::DiscardEdits(1));
    }
}
