use crate::desktop::Desktop;
use crate::desktop::chrome::{Confirm, Overlay};
use crate::status::Status;
use crate::terminals::sessions::Session;
use crate::util::basename;
use agents::Summary;
use gpui_kit::*;

/// What would be cut off by closing a terminal.
#[derive(Clone, Debug, PartialEq)]
pub enum Busy {
    Agent { title: String },
    Shell { command: String },
}

/// An agent is busy only while Working; any other terminal is busy while a command runs in its foreground.
pub fn busy(summary: Option<&Summary>, session: Option<&Session>) -> Option<Busy> {
    match summary.and_then(|a| Some((a, Status::of(a)?))) {
        Some((a, status)) => (status == Status::Working).then(|| Busy::Agent { title: if a.title.is_empty() { a.provider.clone() } else { a.title.clone() } }),
        None => session.and_then(Session::busy).map(|command| Busy::Shell { command: command.to_string() }),
    }
}

impl Desktop {
    /// Asks before closing terminals when one of them is busy, unless the user turned that off; returns whether it asked, so the caller closes them itself otherwise.
    pub(crate) fn ask_close(&mut self, ids: Vec<String>, cx: &mut Context<Self>) -> bool {
        if !self.store.terminal.confirm_close {
            return false;
        }
        let Some((id, busy)) = ids.iter().find_map(|id| Some((id, busy(self.summary(id), self.terminals.sessions.get(id))?))) else { return false };
        let worktree = self.terminals.sessions.get(id).and_then(|s| self.tree_of(&s.info.cwd)).map(|t| basename(&t)).unwrap_or_default();
        self.confirm = Some(Confirm::CloseTerminals { ids, busy, worktree });
        self.overlay = Some(Overlay::Confirm);
        cx.notify();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::{Busy, busy};
    use crate::terminals::sessions::Session;
    use agents::Summary;
    use daemon::Info;

    fn agent(status: &str, attached: bool) -> Summary {
        Summary { title: "Fix login".into(), provider: "claude".into(), status: status.into(), attached, ..Default::default() }
    }

    fn shell(foreground: &str) -> Session {
        Session { info: Info { foreground: foreground.into(), ..Default::default() }, term: None, exit: None }
    }

    #[test]
    fn an_agent_is_busy_only_while_working() {
        let s = shell("claude");
        assert_eq!(busy(Some(&agent("working", true)), Some(&s)), Some(Busy::Agent { title: "Fix login".into() }));
        assert_eq!(busy(Some(&agent("idle", true)), Some(&s)), None);
        assert_eq!(busy(Some(&agent("needsYou", true)), Some(&s)), None);
    }

    #[test]
    fn a_shell_is_busy_while_a_command_runs() {
        assert_eq!(busy(None, Some(&shell("npm test"))), Some(Busy::Shell { command: "npm test".into() }));
        assert_eq!(busy(None, Some(&shell(""))), None);
        assert_eq!(busy(Some(&agent("working", false)), Some(&shell("claude"))), Some(Busy::Shell { command: "claude".into() }));
    }

    #[test]
    fn an_untitled_agent_goes_by_its_provider() {
        let a = agent("working", true);
        let untitled = Summary { title: String::new(), ..a };
        assert_eq!(busy(Some(&untitled), None), Some(Busy::Agent { title: "claude".into() }));
    }
}
