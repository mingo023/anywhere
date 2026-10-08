use crate::actions::Quit;
use crate::desktop::Desktop;
use crate::desktop::chrome::{Confirm, Overlay};
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
}

fn ask(unsaved: usize, alert_up: bool) -> Ask {
    if unsaved > 0 {
        Ask::DiscardEdits(unsaved)
    } else if alert_up {
        Ask::Nothing
    } else {
        Ask::Alert
    }
}

impl Desktop {
    pub fn quit(&mut self, _: &Quit, window: &mut Window, cx: &mut Context<Self>) {
        match ask(self.preview.drafts.len(), ALERT_UP.load(Ordering::Relaxed)) {
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
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Ask, ask};

    #[test]
    fn quitting_asks_about_unsaved_edits_before_the_alert_and_never_stacks_alerts() {
        assert_eq!(ask(2, false), Ask::DiscardEdits(2));
        assert_eq!(ask(0, false), Ask::Alert);
        assert_eq!(ask(0, true), Ask::Nothing);
    }
}
