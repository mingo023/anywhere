//! The checklist a host's row shows while pocketd is installed or updated there.

use crate::install::{Progress, Step};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mark {
    Pending,
    Active,
    Done,
    /// The install had no need of it, as Download when the archive is uploaded.
    Skipped,
    Failed,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    Installed,
    /// A pocketd was already running there.
    Reused,
    Failed(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Checklist {
    pub update: bool,
    marks: [Mark; Step::ALL.len()],
    pub warnings: Vec<String>,
    pub outcome: Option<Outcome>,
}

impl Checklist {
    pub fn new(update: bool) -> Self {
        Self { update, marks: [Mark::Pending; Step::ALL.len()], warnings: Vec::new(), outcome: None }
    }

    /// Records `progress`; what comes after the outcome is dropped.
    pub fn apply(&mut self, progress: Progress) {
        if self.outcome.is_some() {
            return;
        }
        match progress {
            Progress::Step(step) => {
                let at = Step::ALL.iter().position(|s| *s == step).unwrap_or(0);
                self.settle(at);
                self.marks[at] = Mark::Active;
            }
            Progress::Warn(text) => self.warnings.push(text),
            Progress::Running => self.finish(Outcome::Reused),
            Progress::Done => self.finish(Outcome::Installed),
            Progress::Error(text) => self.fail(text),
        }
    }

    /// Fails the step under way, unless the install already ended.
    pub fn fail(&mut self, text: String) {
        if self.outcome.is_some() {
            return;
        }
        for m in self.marks.iter_mut().filter(|m| **m == Mark::Active) {
            *m = Mark::Failed;
        }
        self.outcome = Some(Outcome::Failed(text));
    }

    pub fn succeeded(&self) -> bool {
        matches!(self.outcome, Some(Outcome::Installed | Outcome::Reused))
    }

    /// The steps to show, in order; skipped ones are left out.
    pub fn rows(&self) -> impl Iterator<Item = (Step, Mark)> + '_ {
        Step::ALL.into_iter().zip(self.marks).filter(|(_, m)| *m != Mark::Skipped)
    }

    fn finish(&mut self, outcome: Outcome) {
        self.settle(self.marks.len());
        self.outcome = Some(outcome);
    }

    /// Steps before `at` either ran or were skipped.
    fn settle(&mut self, at: usize) {
        for m in &mut self.marks[..at] {
            *m = if matches!(m, Mark::Active | Mark::Done) { Mark::Done } else { Mark::Skipped };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(progress: &[Progress]) -> Checklist {
        let mut list = Checklist::new(false);
        progress.iter().cloned().for_each(|p| list.apply(p));
        list
    }

    fn marks(list: &Checklist) -> Vec<(Step, Mark)> {
        list.rows().collect()
    }

    #[test]
    fn each_step_ticks_the_one_before_and_an_uploaded_archive_skips_the_download() {
        let list = run(&[Progress::Step(Step::Detect), Progress::Step(Step::Verify)]);
        assert_eq!(marks(&list)[..3], [(Step::Detect, Mark::Done), (Step::Verify, Mark::Active), (Step::Unpack, Mark::Pending)]);
        assert!(marks(&list).iter().all(|(s, _)| *s != Step::Download));
        assert_eq!(list.outcome, None);
    }

    #[test]
    fn a_finished_install_ticks_every_step_it_ran() {
        let steps = [Step::Detect, Step::Verify, Step::Unpack, Step::Link, Step::Service, Step::Start];
        let list = run(&steps.map(Progress::Step).into_iter().chain([Progress::Done]).collect::<Vec<_>>());
        assert_eq!(marks(&list), steps.map(|s| (s, Mark::Done)));
        assert_eq!(list.outcome, Some(Outcome::Installed));
        assert!(list.succeeded());
    }

    #[test]
    fn a_pocketd_already_running_ends_the_list_after_the_check() {
        let list = run(&[Progress::Step(Step::Detect), Progress::Running]);
        assert_eq!(marks(&list), [(Step::Detect, Mark::Done)]);
        assert_eq!(list.outcome, Some(Outcome::Reused));
    }

    #[test]
    fn an_error_fails_the_step_under_way_and_keeps_its_message() {
        let list = run(&[Progress::Step(Step::Detect), Progress::Warn("linger is off".into()), Progress::Step(Step::Service), Progress::Error("systemctl failed".into())]);
        assert_eq!(marks(&list)[..2], [(Step::Detect, Mark::Done), (Step::Service, Mark::Failed)]);
        assert_eq!(list.warnings, ["linger is off"]);
        assert_eq!(list.outcome, Some(Outcome::Failed("systemctl failed".into())));
        assert!(!list.succeeded());
    }

    #[test]
    fn the_first_word_on_how_it_ended_stands() {
        let mut list = run(&[Progress::Step(Step::Start), Progress::Error("pocketd didn't start".into())]);
        list.fail("The install stopped before it finished".into());
        list.apply(Progress::Done);
        assert_eq!(list.outcome, Some(Outcome::Failed("pocketd didn't start".into())));
    }
}
