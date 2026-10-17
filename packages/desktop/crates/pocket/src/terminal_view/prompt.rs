use crate::desktop::Desktop;
use crate::desktop::chrome::id;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::collections::HashMap;
use theme::*;

/// Lines a pinned prompt shows until it is opened, so a long one never takes over the pane.
const FOLDED_LINES: usize = 2;
/// Past this an opened prompt scrolls.
const OPEN_MAX_HEIGHT: f32 = 160.;

/// The panes whose pinned prompt is opened, each with the prompt it was opened on, so the next prompt shows folded.
#[derive(Default)]
pub struct OpenPrompts(HashMap<String, String>);

impl OpenPrompts {
    pub fn is_open(&self, pane: &str, prompt: &str) -> bool {
        self.0.get(pane).is_some_and(|p| p == prompt)
    }

    pub fn toggle(&mut self, pane: &str, prompt: &str) {
        if self.is_open(pane, prompt) {
            self.0.remove(pane);
        } else {
            self.0.insert(pane.to_string(), prompt.to_string());
        }
    }
}

/// A prompt as one paragraph: line breaks and runs of spaces read as one space, so the folded lines hold as much as they can.
pub fn folded(prompt: &str) -> String {
    prompt.split_whitespace().collect::<Vec<_>>().join(" ")
}

impl Desktop {
    /// The latest prompt of the agent in `pane`, pinned above its terminal so it stays in view however far the output runs.
    /// A click opens it in full, and closes it again.
    pub(crate) fn pinned_prompt(&self, pane: &str, cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        let prompt = self.summary(pane).map(|a| a.prompt.clone()).filter(|p| !p.is_empty())?;
        let open = self.terminal.prompts.is_open(pane, &prompt);
        let text = if open { prompt.clone() } else { folded(&prompt) };
        let toggled = pane.to_string();
        Some(
            div()
                .id(id(format!("prompt-{pane}")))
                .flex_none()
                .flex()
                .gap(px(8.))
                .px(px(16.))
                .py(px(6.))
                .border_b(px(0.5))
                .border_color(SEPARATOR)
                .bg(FILL_2)
                .text_size(px(12.))
                .line_height(px(18.))
                .cursor_pointer()
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.terminal.prompts.toggle(&toggled, &prompt);
                    cx.notify();
                }))
                .child(div().flex_none().text_color(TEXT_3).child("❯"))
                .child(
                    div()
                        .id(id(format!("prompt-text-{pane}")))
                        .flex_1()
                        .min_w_0()
                        .text_color(TEXT)
                        .when(open, |d| d.max_h(px(OPEN_MAX_HEIGHT)).overflow_y_scroll())
                        .when(!open, |d| d.line_clamp(FOLDED_LINES))
                        .child(text),
                ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{OpenPrompts, folded};

    #[test]
    fn a_prompt_folds_onto_one_paragraph() {
        assert_eq!(folded("  fix\n\nthe   login\n"), "fix the login");
    }

    #[test]
    fn a_click_opens_the_prompt_and_another_closes_it() {
        let mut open = OpenPrompts::default();
        open.toggle("t1", "fix the login");
        assert!(open.is_open("t1", "fix the login") && !open.is_open("t2", "fix the login"));
        open.toggle("t1", "fix the login");
        assert!(!open.is_open("t1", "fix the login"));
    }

    #[test]
    fn the_next_prompt_shows_folded_and_opens_on_its_own_click() {
        let mut open = OpenPrompts::default();
        open.toggle("t1", "fix the login");
        assert!(!open.is_open("t1", "now the logout"));
        open.toggle("t1", "now the logout");
        assert!(open.is_open("t1", "now the logout"));
    }
}
