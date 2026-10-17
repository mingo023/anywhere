use crate::automations::logic::clock;
use crate::desktop::Desktop;
use crate::desktop::chrome::id;
use agents::{Item, Summary};
use chrono::Local;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::collections::HashMap;
use term::{Cell, WIDE_SPACER_TAIL};
use theme::*;

/// Lines a pinned prompt shows until it is opened, so a long one never takes over the pane.
const FOLDED_LINES: usize = 2;
const SIZE: f32 = 13.;
const LINE: f32 = 19.;
/// Past this an opened prompt scrolls.
const OPEN_MAX_HEIGHT: f32 = 160.;
/// Taken wide on purpose: guessing a prompt overflows only costs a Show more that opens to the same text.
const CHAR_WIDTH: f32 = SIZE * 0.6;
/// The card's inset from the pane, its padding and its border, on both sides.
const CARD_INSET: f32 = 2. * (12. + 12. + 1.);

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

/// A prompt the user sent, and when, in Unix ms.
#[derive(Clone, Debug, PartialEq)]
pub struct Said {
    pub text: String,
    pub at: i64,
}

/// A prompt as one paragraph: line breaks and runs of spaces read as one space, so the folded lines hold as much as they can.
pub fn folded(prompt: &str) -> String {
    prompt.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Whether `prompt`, folded, runs past two lines of a card `width` px wide, or has line breaks only the open card shows.
pub fn overflows(prompt: &str, width: f32) -> bool {
    let per_line = (width / CHAR_WIDTH).floor().max(1.) as usize;
    prompt.trim().contains('\n') || folded(prompt).chars().count() > per_line * FOLDED_LINES
}

/// The prompts Claude Code shows on `cells`, top first, as (row, text shown): the line it pins while scrolled up, and
/// each prompt in the transcript. Both open with "❯" on a background the program paints across the row; the line it
/// edits the next prompt on has none.
pub fn prompt_rows(cells: &[Cell], cols: usize) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    for (row, line) in cells.chunks(cols.max(1)).enumerate() {
        let Some(start) = line.iter().position(|c| c.ch() != ' ') else { continue };
        let painted = line.iter().filter(|c| c.has_bg == 1).count();
        if line[start].ch() != '❯' || line[start].has_bg == 0 || painted * 2 < line.len() {
            continue;
        }
        let text: String = line[start + 1..].iter().filter(|c| c.wide != WIDE_SPACER_TAIL).map(Cell::ch).collect();
        let text = text.trim().trim_end_matches('…').trim_end();
        if !text.is_empty() {
            out.push((row, text.to_string()));
        }
    }
    out
}

/// What the user sent in `agent`'s conversation, oldest first: the prompts in its timeline, then the latest pocketd
/// reports if the timeline hasn't caught up with it.
pub fn said(timeline: Option<&Vec<Item>>, agent: &Summary) -> Vec<Said> {
    let mut said: Vec<Said> = timeline.into_iter().flatten().filter(|i| i.kind == "user" && !i.text.trim().is_empty()).map(|i| Said { text: i.text.trim().to_string(), at: i.ts }).collect();
    let latest = agent.prompt.trim_end_matches('…');
    if !latest.is_empty() && !said.last().is_some_and(|s| s.text.starts_with(latest)) {
        said.push(Said { text: agent.prompt.clone(), at: agent.prompt_at });
    }
    said
}

/// The prompt whose turn the top of the screen is in, given the prompts on screen (`prompt_rows`) and all `said`:
/// the one at the top row, else the one before the first on screen, else the latest. None above the first prompt.
pub fn at_top<'a>(shown: &[(usize, String)], said: &'a [Said]) -> Option<&'a Said> {
    let find = |text: &str| said.iter().rposition(|s| folded(&s.text).starts_with(&folded(text)));
    match shown.first() {
        None => said.last(),
        Some((row, text)) => match (find(text), *row == 0) {
            (Some(i), true) => said.get(i),
            (Some(i), false) => i.checked_sub(1).and_then(|i| said.get(i)),
            (None, _) => said.last(),
        },
    }
}

impl Desktop {
    /// The prompt whose turn the top of `pane`'s output is in, floated over it as VS Code's Claude panel pins it, so
    /// it stays in view however far the output runs. Folded to two lines, its second fades out under Show more.
    /// `shown` is what `prompt_rows` read off the screen; `width` is the output's, which the card spans.
    pub(crate) fn pinned_prompt(&self, pane: &str, shown: &[(usize, String)], width: f32, cx: &mut Context<Self>) -> Option<Div> {
        let agent = self.summary(pane)?;
        let said = said(self.agents.timelines.get(&agent.id), agent);
        let top = at_top(shown, &said)?.clone();
        let open = self.terminal.prompts.is_open(pane, &top.text);
        let long = overflows(&top.text, width - CARD_INSET);
        let text = if open { top.text.clone() } else { folded(&top.text) };
        // Opaque, so the output scrolling under the card never shows through it.
        let card = Hsla::from(POPOVER).blend(FILL_3.into());
        let strip = if is_dark() { WINDOW_SOLID } else { SURFACE_SUNKEN };
        let (toggled, prompt) = (pane.to_string(), top.text.clone());
        let more = ui::control(div().id(id(format!("prompt-more-{pane}"))))
            .absolute()
            .right(px(6.))
            .bottom(px(6.))
            .h(px(22.))
            .px(px(8.))
            .flex()
            .items_center()
            .rounded(px(6.))
            .bg(TEXT_BODY)
            .text_color(ON_TEXT)
            .text_size(px(12.))
            .cursor_pointer()
            .hover(|s| s.bg(TEXT))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.terminal.prompts.toggle(&toggled, &prompt);
                cx.notify();
            }))
            .child(if open { "Show less" } else { "Show more" });
        Some(
            div()
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .px(px(12.))
                .pt(px(4.))
                .font_family(ui_font())
                .bg(linear_gradient(180., linear_color_stop(strip, 0.6), linear_color_stop(Hsla::from(strip).opacity(0.), 1.)))
                .children((top.at > 0).then(|| div().flex().justify_end().pb(px(4.)).text_size(px(11.)).line_height(px(14.)).text_color(TEXT_3).child(clock(top.at, &Local))))
                .child(
                    div()
                        .id(id(format!("prompt-{pane}")))
                        .occlude()
                        .relative()
                        .px(px(12.))
                        .py(px(8.))
                        .rounded(px(8.))
                        .border_1()
                        .border_color(SEPARATOR_STRONG)
                        .bg(card)
                        .shadow(vec![ui::shadow(rgba(0x00000059), 4., 14.), ui::shadow(rgba(0x00000026), 1., 3.)])
                        .text_size(px(SIZE))
                        .line_height(px(LINE))
                        .text_color(TEXT)
                        .child(
                            div()
                                .id(id(format!("prompt-text-{pane}")))
                                .when(open, |d| d.max_h(px(OPEN_MAX_HEIGHT)).overflow_y_scroll().pb(px(22.)))
                                .when(!open, |d| d.line_clamp(FOLDED_LINES))
                                .child(text),
                        )
                        .when(long && !open, |d| {
                            d.child(div().absolute().left_0().right_0().bottom_0().h(px(LINE + 8.)).rounded_b(px(8.)).bg(linear_gradient(
                                180.,
                                linear_color_stop(card.opacity(0.), 0.),
                                linear_color_stop(card, 0.9),
                            )))
                        })
                        .when(long, |d| d.child(more)),
                ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{OpenPrompts, Said, at_top, folded, overflows, prompt_rows, said};
    use agents::{Item, Summary};
    use term::Cell;

    fn said_at(texts: &[&str]) -> Vec<Said> {
        texts.iter().enumerate().map(|(n, t)| Said { text: t.to_string(), at: n as i64 }).collect()
    }

    /// Rows of `cols` cells; a row given as `(text, true)` has its background painted across it.
    fn screen(cols: usize, rows: &[(&str, bool)]) -> Vec<Cell> {
        rows.iter()
            .flat_map(|(text, painted)| {
                let mut chars: Vec<char> = text.chars().collect();
                chars.resize(cols, ' ');
                chars.into_iter().map(|ch| Cell { cp: ch as u32, has_bg: *painted as u8, ..Default::default() }).collect::<Vec<_>>()
            })
            .collect()
    }

    #[test]
    fn a_prompt_folds_onto_one_paragraph() {
        assert_eq!(folded("  fix\n\nthe   login\n"), "fix the login");
    }

    #[test]
    fn show_more_appears_past_two_lines_or_for_line_breaks() {
        let width = 13. * 0.6 * 10.;
        assert!(!overflows(&"x".repeat(20), width));
        assert!(overflows(&"x".repeat(21), width));
        assert!(overflows("one\ntwo", width));
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

    #[test]
    fn prompts_on_screen_are_painted_rows_opening_with_the_pointer() {
        let cells = screen(20, &[("❯ pinned while scr…", true), ("● reply", false), ("  ❯ second prompt", true), ("❯ being typed", false)]);
        assert_eq!(prompt_rows(&cells, 20), [(0, "pinned while scr".to_string()), (2, "second prompt".to_string())]);
    }

    #[test]
    fn the_top_row_names_the_prompt_in_full() {
        let all = said_at(&["one", "two is long\nand wraps", "three"]);
        assert_eq!(at_top(&[(0, "two is".into())], &all), Some(&all[1]));
    }

    #[test]
    fn a_prompt_below_the_top_means_the_one_before_it_is_still_answering() {
        let all = said_at(&["one", "two", "three"]);
        assert_eq!(at_top(&[(7, "three".into())], &all), Some(&all[1]));
        assert_eq!(at_top(&[(7, "one".into())], &all), None);
    }

    #[test]
    fn with_no_prompt_on_screen_or_none_known_the_latest_is_pinned() {
        let all = said_at(&["one", "two"]);
        assert_eq!(at_top(&[], &all), Some(&all[1]));
        assert_eq!(at_top(&[(3, "never sent".into())], &all), Some(&all[1]));
        assert_eq!(at_top(&[], &[]), None);
    }

    #[test]
    fn the_latest_prompt_joins_a_timeline_that_has_not_caught_up() {
        let user = |text: &str, ts| Item { kind: "user".into(), text: text.into(), ts, ..Default::default() };
        let items = vec![user("one", 1), Item { kind: "assistant".into(), text: "hi".into(), ..Default::default() }, user("two", 2)];
        let caught_up = Summary { prompt: "two".into(), prompt_at: 2, ..Default::default() };
        assert_eq!(said(Some(&items), &caught_up), [Said { text: "one".into(), at: 1 }, Said { text: "two".into(), at: 2 }]);
        let ahead = Summary { prompt: "three".into(), prompt_at: 3, ..Default::default() };
        assert_eq!(said(Some(&items), &ahead).last(), Some(&Said { text: "three".into(), at: 3 }));
    }
}
