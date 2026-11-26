//! The transcript's side of running a shell block: starting and stopping a run, its events landing, and its card.

use super::{View, ViewEvent};
use crate::chat::run_card::{self, ARMED, Run};
use crate::chat::runner::Event;
use futures::StreamExt;
use gpui_kit::*;
use std::collections::HashMap;
use std::time::{Duration, Instant};

#[derive(Default)]
pub(super) struct Runs {
    by_row: HashMap<String, Run>,
    /// The risky block whose Run was pressed once, and when.
    armed: Option<(String, Instant)>,
    _tasks: HashMap<String, Task<()>>,
}

impl Runs {
    pub(super) fn get(&self, row: &str) -> Option<&Run> {
        self.by_row.get(row)
    }

    pub(super) fn armed(&self, row: &str, now: Instant) -> bool {
        self.armed.as_ref().is_some_and(|(r, at)| r == row && now.saturating_duration_since(*at) < ARMED)
    }
}

impl View {
    /// Runs block `row`'s steps where its agent works, or stops the run going; a risky block runs on its second press.
    pub(super) fn press_run(&mut self, row: &str, steps: Vec<String>, cx: &mut Context<Self>) {
        let Some(cwd) = self.side.runs_in.clone() else { return };
        let now = Instant::now();
        if let Some(run) = self.runs.by_row.get_mut(row).filter(|r| r.running()) {
            run.stop();
            return cx.notify();
        }
        if steps.iter().any(|s| crate::chat::steps::risky(s)) && !self.runs.armed(row, now) {
            self.runs.armed = Some((row.to_string(), now));
            let row = row.to_string();
            self.runs._tasks.insert(
                format!("{row}#armed"),
                cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(ARMED).await;
                    _ = this.update(cx, |v, cx| {
                        if v.runs.armed.as_ref().is_some_and(|(r, _)| *r == row) {
                            v.runs.armed = None;
                            cx.notify();
                        }
                    });
                }),
            );
            return cx.notify();
        }
        self.runs.armed = None;
        let (run, mut events) = run_card::start(steps, cwd);
        self.runs.by_row.insert(row.to_string(), run);
        let key = row.to_string();
        let task = cx.spawn(async move |this, cx| {
            let mut tick = Instant::now();
            loop {
                let next = futures::future::select(events.next(), Box::pin(cx.background_executor().timer(Duration::from_secs(1)))).await;
                let event = match next {
                    futures::future::Either::Left((Some(event), _)) => Some(event),
                    futures::future::Either::Left((None, _)) => break,
                    futures::future::Either::Right(_) => None,
                };
                let now = Instant::now();
                let landed = this.update(cx, |v, cx| {
                    let Some(run) = v.runs.by_row.get_mut(&key) else { return };
                    let output = matches!(event, Some(Event::Output(_)));
                    if let Some(event) = event {
                        run.apply(event);
                    }
                    // Output can pour in, so the card redraws for it at most every 50ms; a step, the end and the clock redraw it at once.
                    if !output || now.saturating_duration_since(tick) >= Duration::from_millis(50) {
                        tick = now;
                        run.follow();
                        v.remeasure_row(&key);
                        cx.notify();
                    }
                });
                if landed.is_err() {
                    break;
                }
            }
        });
        self.runs._tasks.insert(row.to_string(), task);
        cx.notify();
    }

    pub(super) fn hide_run(&mut self, row: &str, cx: &mut Context<Self>) {
        if let Some(run) = self.runs.by_row.get_mut(row) {
            run.shown = !run.shown;
            self.remeasure_row(row);
            cx.notify();
        }
    }

    pub(super) fn ask_fix(&mut self, row: &str, cx: &mut Context<Self>) {
        if let Some(prompt) = self.runs.by_row.get(row).and_then(Run::fix_prompt) {
            cx.emit(ViewEvent::AskFix(prompt));
        }
    }

    fn remeasure_row(&self, row: &str) {
        if let Some(ix) = self.session.ids.iter().position(|i| i == row) {
            self.list.remeasure_items(ix..ix + 1);
        }
    }
}
