use crate::adapter::{Backend, Capabilities};
use crate::{Ask, Fold, Transcript, Turn};

pub struct Claude;

impl Backend for Claude {
    type Item = agents::Item;
    type Ask = agents::Permission;

    fn capabilities(&self) -> Capabilities {
        Capabilities { interrupt: true, compact: true, send_while_working: true, sub_agents: true }
    }

    fn transcript(&self, items: &[agents::Item]) -> Transcript {
        crate::timeline::fold(items)
    }

    fn refold(&self, items: &[agents::Item], t: &mut Transcript, fold: &mut Fold) -> Vec<Turn> {
        fold.refold(items, t)
    }

    fn ask(&self, ask: &agents::Permission) -> Ask {
        crate::timeline::ask(ask)
    }

    fn panel<'t>(&self, text: &'t str) -> Option<&'t str> {
        let mut words = text.split_whitespace();
        let command = words.next()?;
        let name = command.strip_prefix('/')?;
        let bare = words.next().is_none();
        (PANELS.contains(&name) || bare && PICKERS.contains(&name)).then_some(command)
    }
}

/// Claude's commands that open a panel, by name and alias.
const PANELS: &[&str] = &[
    "agents", "bug", "checkpoint", "checkup", "config", "cost", "diff", "doctor", "export", "feedback", "help", "hooks", "ide", "login", "logout",
    "mcp", "memory", "permissions", "plugin", "privacy-settings", "rewind", "settings", "skills", "stats", "status", "tasks", "terminal-setup",
    "theme", "usage",
];

/// Claude's commands that open a picker unless given what to pick.
const PICKERS: &[&str] = &["add-dir", "effort", "model", "resume"];
