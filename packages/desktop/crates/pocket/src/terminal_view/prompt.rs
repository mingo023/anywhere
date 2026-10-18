use crate::desktop::Desktop;
use crate::desktop::chrome::id;
use crate::terminal_view::surface::Metrics;
use agents::{Item, Summary};
use chrono::{DateTime, Local};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::collections::HashMap;
use std::time::Duration;
use term::{Button, Cell, MouseAction, Pointer, WIDE_SPACER_TAIL};
use theme::*;

/// Lines a pinned prompt shows until it is opened, so a long one never takes over the pane.
const FOLDED_LINES: usize = 2;
const SIZE: f32 = 14.;
const LINE: f32 = 20.;
const PAD_X: f32 = 12.;
const PAD_Y: f32 = 10.;
const RADIUS: f32 = 8.;
/// The row under a cut-off prompt that holds Show more.
const MORE_ROW: f32 = 26.;
/// Past this an opened prompt scrolls.
const OPEN_MAX_HEIGHT: f32 = 160.;
/// Taken wide on purpose: guessing a prompt overflows only costs a Show more that opens to the same text.
const CHAR_WIDTH: f32 = SIZE * 0.6;
/// The card's inset from the pane, its padding and its border, on both sides.
const CARD_INSET: f32 = 2. * (12. + PAD_X + 1.);

/// A Mono cell is 0.6em wide.
const MONO_CELL: f32 = 0.6;
/// How long a jump waits for Claude Code to pin the prompt line once it scrolls, a check every `JUMP_POLL`.
const JUMP_TRIES: usize = 10;
const JUMP_POLL: Duration = Duration::from_millis(50);

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

/// A prompt as one paragraph: line breaks and runs of spaces read as one space, so the one line Claude Code shows of
/// a prompt matches the prompt it came from.
pub fn folded(prompt: &str) -> String {
    prompt.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Whether `prompt` wraps past two lines of a card `width` px wide, so the card cuts it off.
pub fn overflows(prompt: &str, width: f32) -> bool {
    let per_line = (width / CHAR_WIDTH).floor().max(1.) as usize;
    prompt.trim().lines().map(|l| l.chars().count().div_ceil(per_line).max(1)).sum::<usize>() > FOLDED_LINES
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
    /// it stays in view however far the output runs. Cut to two lines, the second fades out above Show more.
    /// A click on the card scrolls Claude Code to the prompt. `shown` is what `prompt_rows` read off the screen.
    pub(crate) fn pinned_prompt(&self, pane: &str, shown: &[(usize, String)], cols: u16, m: &Metrics, cx: &mut Context<Self>) -> Option<Div> {
        let (cell, line) = (m.size * MONO_CELL, m.line);
        let width = cols as f32 * cell;
        let agent = self.summary(pane)?;
        let prompts = agents::providers::get(&agent.provider).is_some_and(|b| b.terminal_prompts);
        let said = said(self.agents.timelines.get(&agent.id), agent);
        let top = at_top(shown, &said)?.clone();
        let open = self.terminal.prompts.is_open(pane, &top.text);
        let long = overflows(&top.text, width - CARD_INSET);
        // Opaque and lifted off the output, which scrolls under it.
        let card = Hsla::from(POPOVER).blend(FILL_4.into()).blend(FILL_2.into());
        let (toggled, prompt) = (pane.to_string(), top.text.clone());
        let more = ui::control(div().id(id(format!("prompt-more-{pane}"))))
            .absolute()
            .right_0()
            .bottom_0()
            .h(px(MORE_ROW))
            .px(px(10.))
            .flex()
            .items_center()
            .rounded_tl(px(6.))
            .rounded_br(px(RADIUS - 1.))
            .bg(TEXT_BODY)
            .text_color(ON_TEXT)
            .text_size(px(13.))
            .cursor_pointer()
            .hover(|s| s.bg(TEXT))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                this.terminal.prompts.toggle(&toggled, &prompt);
                cx.notify();
            }))
            .child(if open { "Show less" } else { "Show more" });
        let time = (top.at > 0).then(|| {
            div()
                .absolute()
                .top(px(-20.))
                .right(px(10.))
                .px(px(6.))
                .rounded(px(4.))
                .bg(card)
                .text_size(px(12.))
                .line_height(px(16.))
                .text_color(TEXT_2)
                .child(DateTime::from_timestamp_millis(top.at).unwrap_or_default().with_timezone(&Local).format("%-I:%M %p").to_string())
        });
        Some(
            div().absolute().top_0().left_0().right_0().px(px(12.)).pt(px(24.)).font_family(ui_font()).child(
                div()
                    .id(id(format!("prompt-{pane}")))
                    .occlude()
                    .relative()
                    .px(px(PAD_X))
                    .pt(px(PAD_Y))
                    .pb(px(if long { MORE_ROW } else { PAD_Y }))
                    .rounded(px(RADIUS))
                    .border_1()
                    .border_color(SEPARATOR_STRONG)
                    .bg(card)
                    .shadow(vec![ui::shadow(rgba(0x00000066), 6., 18.), ui::shadow(rgba(0x00000033), 1., 4.)])
                    .text_size(px(SIZE))
                    .line_height(px(LINE))
                    .text_color(TEXT)
                    .when(prompts, |d| {
                        let jumped = pane.to_string();
                        d.cursor_pointer().on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.jump_to_prompt(&jumped, cell, line, cx)))
                    })
                    .child(
                        div()
                            .id(id(format!("prompt-text-{pane}")))
                            .when(open, |d| d.max_h(px(OPEN_MAX_HEIGHT)).overflow_y_scroll())
                            .when(!open, |d| d.line_clamp(FOLDED_LINES))
                            .child(top.text.clone()),
                    )
                    .when(long && !open, |d| {
                        d.child(div().absolute().left_0().right_0().top(px(PAD_Y + LINE)).h(px(LINE)).bg(linear_gradient(
                            180.,
                            linear_color_stop(card.opacity(0.2), 0.),
                            linear_color_stop(card.opacity(0.85), 1.),
                        )))
                    })
                    .children(time)
                    .when(long, |d| d.child(more)),
            ),
        )
    }

    /// Scrolls `pane`'s Claude Code to the prompt its card pins, as a click on the card does in VS Code. The click goes
    /// to the prompt line Claude pins on the top row while scrolled up, which jumps there. At the bottom there is no
    /// such line yet, so a wheel notch up brings it and the click follows once it shows.
    pub(crate) fn jump_to_prompt(&mut self, pane: &str, cell: f32, line: f32, cx: &mut Context<Self>) {
        if self.click_top_prompt(pane, cell, line) {
            return;
        }
        let Some(t) = self.terminals.sessions.term(pane) else { return };
        let (cols, rows) = {
            let (f, _) = t.frame();
            (f.cols, f.rows)
        };
        let at = Pointer::at(cell * 4.5, line * (rows / 2) as f32, cell, line, cols, rows);
        let bytes = t.wheel_report(at, true, false, false);
        if bytes.is_empty() {
            return;
        }
        self.send_input(pane, &bytes, cx);
        let pane = pane.to_string();
        cx.spawn(async move |this, cx| {
            for _ in 0..JUMP_TRIES {
                cx.background_executor().timer(JUMP_POLL).await;
                if this.update(cx, |d, _| d.click_top_prompt(&pane, cell, line)).unwrap_or(true) {
                    break;
                }
            }
        })
        .detach();
    }

    /// Clicks the prompt line on `pane`'s top row; false while there is none. A pane that's gone counts as done.
    fn click_top_prompt(&mut self, pane: &str, cell: f32, line: f32) -> bool {
        let Some(t) = self.terminals.sessions.term(pane) else { return true };
        let (on_top, cols, rows) = {
            let (f, cells) = t.frame();
            (prompt_rows(cells, f.cols as usize).first().is_some_and(|(row, _)| *row == 0), f.cols, f.rows)
        };
        if !on_top {
            return false;
        }
        let at = Pointer::at(cell * 4.5, line * 0.5, cell, line, cols, rows);
        for action in [MouseAction::Press, MouseAction::Release] {
            self.send_mouse(pane, at, action, Some(Button::Left), Modifiers::default());
        }
        true
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
    fn show_more_appears_only_once_the_prompt_wraps_past_two_lines() {
        let width = super::CHAR_WIDTH * 10.;
        assert!(!overflows(&"x".repeat(20), width));
        assert!(overflows(&"x".repeat(21), width));
        assert!(!overflows("one\ntwo\n", width));
        assert!(overflows("one\ntwo\nthree", width));
        assert!(overflows(&format!("one\n{}", "x".repeat(11)), width));
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
