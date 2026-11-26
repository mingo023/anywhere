//! A shell block's Run button, and the card under the block its run streams into.

use super::runner::{self, Event, Exit, Stop};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::time::{Duration, Instant};
use theme::*;

/// Output kept per run: a long build's start goes, its end stays.
const KEEP: usize = 64 * 1024;
/// How long a risky block's Run waits for its second press.
pub(super) const ARMED: Duration = Duration::from_secs(3);
const SWEEP: Duration = Duration::from_millis(1300);
const OUTPUT_MAX_H: f32 = 198.;

/// One run of a block: its steps, how far it got, what it printed and how it ended.
pub(super) struct Run {
    steps: Vec<String>,
    step: usize,
    started: Instant,
    ended: Option<Instant>,
    output: String,
    exit: Option<Exit>,
    stop: Option<Stop>,
    /// Whether its card shows the output; hiding it keeps the run and the status line.
    pub(super) shown: bool,
    scroll: ScrollHandle,
}

impl Run {
    pub(super) fn start(steps: Vec<String>, stop: Stop) -> Self {
        Self {
            steps,
            step: 0,
            started: Instant::now(),
            ended: None,
            output: String::new(),
            exit: None,
            stop: Some(stop),
            shown: true,
            scroll: ScrollHandle::new(),
        }
    }

    pub(super) fn running(&self) -> bool {
        self.exit.is_none()
    }

    pub(super) fn stop(&mut self) {
        if let Some(stop) = &self.stop {
            stop.stop();
        }
    }

    pub(super) fn apply(&mut self, event: Event) {
        match event {
            Event::Step(n) => {
                self.step = n;
                if n > 0 {
                    self.output.push('\n');
                }
                self.output.push_str(&format!("$ {}\n", self.steps[n]));
            }
            Event::Output(text) => self.output.push_str(&text),
            Event::Done(exit) => {
                (self.exit, self.ended, self.stop) = (Some(exit), Some(Instant::now()), None);
            }
        }
        if self.output.len() > KEEP {
            let mut cut = self.output.len() - KEEP;
            while !self.output.is_char_boundary(cut) {
                cut += 1;
            }
            self.output.drain(..cut);
        }
    }

    pub(super) fn output(&self) -> &str {
        &self.output
    }

    /// Keeps the output's newest line in view as it streams, unless the user scrolled up to read.
    pub(super) fn follow(&self) {
        if self.scroll.offset().y <= px(24.) - self.scroll.max_offset().y {
            self.scroll.scroll_to_bottom();
        }
    }

    /// "Step 2 of 3 · Working · 4s" while it runs, then how it ended.
    pub(super) fn status(&self, now: Instant) -> String {
        let took = self.ended.unwrap_or(now).saturating_duration_since(self.started).as_secs_f32();
        match &self.exit {
            None => {
                let step = if self.steps.len() > 1 { format!("Step {} of {} · ", self.step + 1, self.steps.len()) } else { String::new() };
                format!("{step}Working · {}s", took as u64)
            }
            Some(Exit::Passed) => format!("Exited 0 in {took:.1}s"),
            Some(Exit::Stopped) => format!("Stopped after {took:.1}s"),
            Some(Exit::Failed { step, code }) => {
                let what = self.steps[*step].split_whitespace().take(2).collect::<Vec<_>>().join(" ");
                match code {
                    Some(code) => format!("{what} exited {code} in {took:.1}s"),
                    None => format!("{what} was killed after {took:.1}s"),
                }
            }
            Some(Exit::Unstarted(why)) => format!("Couldn't start: {why}"),
        }
    }

    /// What to ask the agent once a step failed: the command, its code and the end of what it printed.
    pub(super) fn fix_prompt(&self) -> Option<String> {
        let Some(Exit::Failed { step, code }) = &self.exit else { return None };
        let tail: Vec<&str> = self.output.lines().rev().take(20).collect::<Vec<_>>().into_iter().rev().collect();
        let code = code.map_or("was killed".to_string(), |c| format!("exited {c}"));
        Some(format!("`{}` {code}:\n```\n{}\n```\nFix it?", self.steps[*step], tail.join("\n")))
    }
}

/// The Run button in a shell block's header: Stop while it runs, and a risky block's "Run…" turning red for its second press.
pub(super) fn button(id: &str, running: bool, risky: bool, armed: bool, on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Stateful<Div> {
    let (glyph, label, fg, bg) = match (running, armed) {
        (true, _) => ("stop", "Stop", FAILED, None),
        (false, true) => ("warning", "Press again to run", WHITE, Some(FAILED)),
        (false, false) => ("play", if risky { "Run…" } else { "Run" }, TEXT, None),
    };
    let b = div()
        .id(SharedString::from(format!("run-{id}")))
        .flex_none()
        .h(px(26.))
        .ml(px(6.))
        .pl(px(8.))
        .pr(px(10.))
        .flex()
        .items_center()
        .gap(px(6.))
        .rounded(px(7.))
        .text_size(px(12.5))
        .font_weight(FontWeight::MEDIUM)
        .text_color(fg)
        .cursor_pointer()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(on_click)
        .child(icon(glyph, 11., fg))
        .child(label);
    match bg {
        Some(bg) => b.bg(bg),
        None => b.bg(SURFACE).border_1().border_color(SEPARATOR_STRONG).hover(|s| s.bg(FILL_2)),
    }
}

/// The sweep along the card's top while its run goes: it says something happens without guessing how far along. It sits by
/// the output rather than on the block's top edge, which a transcript held to its bottom scrolls away as the card grows.
fn sweep(id: &str) -> Div {
    div().absolute().top_0().left(px(12.)).right(px(12.)).h(px(2.)).overflow_hidden().child(
        div()
            .absolute()
            .top_0()
            .h_full()
            .w(relative(0.36))
            .flex()
            .child(div().flex_1().h_full().bg(linear_gradient(90., linear_color_stop(Hsla::from(ACCENT).opacity(0.), 0.), linear_color_stop(ACCENT, 1.))))
            .child(div().flex_1().h_full().bg(linear_gradient(90., linear_color_stop(ACCENT, 0.), linear_color_stop(Hsla::from(ACCENT).opacity(0.), 1.))))
            .with_animation(SharedString::from(format!("run-sweep-{id}")), Animation::new(SWEEP).repeat_synced().with_max_fps(20.), |d, t| {
                d.left(relative(-0.36 + t * 1.36))
            }),
    )
}

/// The card under a block: a sweep while it runs, what it printed, and how it ended with what to do next.
pub(super) fn card<C, A, H>(id: &str, run: &Run, copy: C, copied: bool, fix: Option<(String, A)>, hide: H, now: Instant) -> Div
where
    C: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    A: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    H: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
{
    let (lead, colour): (AnyElement, Token) = match run.exit {
        None => (spinner(SharedString::from(format!("run-spin-{id}")), 12., TEXT_3).into_any_element(), TEXT_3),
        Some(Exit::Passed) => (icon("check", 13., SUCCESS_TEXT).into_any_element(), SUCCESS_TEXT),
        Some(Exit::Stopped) => (icon("stop", 11., TEXT_3).into_any_element(), TEXT_3),
        Some(_) => (icon("x", 12., FAILED).into_any_element(), FAILED),
    };
    let link = |key: &str, label: &str, accent: bool| {
        div()
            .id(SharedString::from(format!("run-{key}-{id}")))
            .flex_none()
            .h(px(24.))
            .px(px(8.))
            .flex()
            .items_center()
            .gap(px(5.))
            .rounded(px(6.))
            .text_color(if accent { ACCENT_LINK } else { TEXT_2 })
            .cursor_pointer()
            .hover(|s| s.bg(FILL_2))
            .child(label.to_string())
    };
    let output = (run.shown && !run.output().is_empty()).then(|| {
        div()
            .id(SharedString::from(format!("run-out-{id}")))
            .mx(px(12.))
            .max_h(px(OUTPUT_MAX_H))
            .overflow_y_scroll()
            .track_scroll(&run.scroll)
            .rounded(px(8.))
            .bg(FILL_4)
            .px(px(10.))
            .py(px(8.))
            .font_family(code_font())
            .text_size(px(12.5))
            .line_height(px(18.))
            .text_color(TEXT_3)
            .child(run.output().trim_end().to_string())
    });
    let shown = run.shown;
    let status = div()
        .h(px(40.))
        .pl(px(16.))
        .pr(px(8.))
        .flex()
        .items_center()
        .gap(px(6.))
        .text_size(px(12.5))
        .text_color(TEXT_3)
        .child(div().flex_1().min_w_0().flex().items_center().gap(px(6.)).text_color(colour).child(lead).child(div().truncate().child(run.status(now))))
        .children(fix.map(|(label, ask)| link("fix", &label, true).on_click(ask)))
        .when(!run.running(), |d| d.child(link("copy", if copied { "Copied" } else { "Copy output" }, false).on_click(copy)))
        .child(
            ui::icon_button_sized(SharedString::from(format!("run-hide-{id}")), if run.shown { "chevron-up" } else { "chevron-down" }, 24., TEXT_3)
                .tooltip(move |window, cx| Tooltip::new(if shown { "Hide output" } else { "Show output" }).build(window, cx))
                .on_click(hide),
        );
    div()
        .relative()
        .mt(px(-4.))
        .pt(px(8.))
        .rounded_b(px(15.))
        .border_1()
        .border_t_0()
        .border_color(MD_CODE_BORDER)
        .bg(MD_CODE_BG)
        .children(run.running().then(|| sweep(id)))
        .children(output)
        .child(status)
}

pub(super) fn start(steps: Vec<String>, cwd: std::path::PathBuf) -> (Run, futures::channel::mpsc::Receiver<Event>) {
    let (tx, rx) = futures::channel::mpsc::channel(64);
    let stop = runner::start(steps.clone(), cwd, tx);
    (Run::start(steps, stop), rx)
}

#[cfg(test)]
mod tests {
    use super::{Event, Exit, KEEP, Run, runner};
    use std::time::{Duration, Instant};

    fn run(steps: &[&str]) -> Run {
        Run::start(steps.iter().map(|s| s.to_string()).collect(), runner::start(Vec::new(), std::env::temp_dir(), futures::channel::mpsc::channel(1).0))
    }

    #[test]
    fn a_run_of_several_steps_says_which_one_runs() {
        let mut r = run(&["npm ci", "npm test"]);
        r.apply(Event::Step(1));
        assert_eq!(r.status(r.started + Duration::from_millis(4200)), "Step 2 of 2 · Working · 4s");
    }

    #[test]
    fn a_failed_step_names_its_command_and_code() {
        let mut r = run(&["cargo test -p pocket"]);
        r.apply(Event::Step(0));
        r.apply(Event::Output("error[E0425]: cannot find value\n".into()));
        r.apply(Event::Done(Exit::Failed { step: 0, code: Some(101) }));
        assert!(r.status(Instant::now()).starts_with("cargo test exited 101 in "));
        assert_eq!(r.fix_prompt().unwrap(), "`cargo test -p pocket` exited 101:\n```\n$ cargo test -p pocket\nerror[E0425]: cannot find value\n```\nFix it?");
    }

    #[test]
    fn a_long_run_keeps_the_end_of_what_it_printed() {
        let mut r = run(&["yes"]);
        r.apply(Event::Output("é".repeat(KEEP)));
        r.apply(Event::Output("end".into()));
        assert!(r.output().len() <= KEEP + 1 && r.output().ends_with("end"));
    }
}
