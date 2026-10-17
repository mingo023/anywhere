use crate::automations::logic::clock;
use crate::desktop::Desktop;
use crate::desktop::chrome::id;
use chrono::Local;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::collections::HashMap;
use theme::*;

/// Lines a pinned prompt shows until it is opened, so a long one never takes over the pane.
const FOLDED_LINES: usize = 2;
const LINE: f32 = 19.;
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
    /// The latest prompt of the agent in `pane`, floated over the top of its output as VS Code's Claude panel pins it,
    /// so it stays in view however far the output runs. Folded, its second line fades out; a click opens it in full.
    pub(crate) fn pinned_prompt(&self, pane: &str, cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        let agent = self.summary(pane).filter(|a| !a.prompt.is_empty())?;
        let prompt = agent.prompt.clone();
        let open = self.terminal.prompts.is_open(pane, &prompt);
        let text = if open { prompt.clone() } else { folded(&prompt) };
        // Opaque, so the output scrolling under the card never shows through it.
        let card = Hsla::from(POPOVER).blend(FILL_3.into());
        let strip = if is_dark() { WINDOW_SOLID } else { SURFACE_SUNKEN };
        let toggled = pane.to_string();
        Some(
            div()
                .id(id(format!("prompt-{pane}")))
                .occlude()
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .px(px(12.))
                .pt(px(4.))
                .font_family(ui_font())
                .bg(linear_gradient(180., linear_color_stop(strip, 0.6), linear_color_stop(Hsla::from(strip).opacity(0.), 1.)))
                .children((agent.prompt_at > 0).then(|| div().flex().justify_end().pb(px(4.)).text_size(px(11.)).line_height(px(14.)).text_color(TEXT_3).child(clock(agent.prompt_at, &Local))))
                .child(
                    div()
                        .id(id(format!("prompt-card-{pane}")))
                        .relative()
                        .px(px(12.))
                        .py(px(8.))
                        .rounded(px(8.))
                        .border_1()
                        .border_color(SEPARATOR_STRONG)
                        .bg(card)
                        .shadow(vec![ui::shadow(rgba(0x00000059), 4., 14.), ui::shadow(rgba(0x00000026), 1., 3.)])
                        .text_size(px(13.))
                        .line_height(px(LINE))
                        .text_color(TEXT)
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.terminal.prompts.toggle(&toggled, &prompt);
                            cx.notify();
                        }))
                        .child(
                            div()
                                .id(id(format!("prompt-text-{pane}")))
                                .when(open, |d| d.max_h(px(OPEN_MAX_HEIGHT)).overflow_y_scroll())
                                .when(!open, |d| d.h(px(LINE * FOLDED_LINES as f32)).line_clamp(FOLDED_LINES))
                                .child(text),
                        )
                        .when(!open, |d| {
                            d.child(div().absolute().left(px(12.)).right(px(12.)).bottom(px(8.)).h(px(LINE * 0.8)).bg(linear_gradient(
                                180.,
                                linear_color_stop(card.opacity(0.), 0.),
                                linear_color_stop(card, 1.),
                            )))
                        }),
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
