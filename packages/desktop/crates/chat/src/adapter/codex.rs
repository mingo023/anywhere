use crate::adapter::{Backend, Capabilities};
use crate::commands::{Builtin, Kind, Local};
use crate::{Ask, Fold, Transcript, Turn};

pub struct Codex;

impl Backend for Codex {
    type Item = agents::Item;
    type Ask = agents::Permission;

    fn capabilities(&self) -> Capabilities {
        Capabilities { interrupt: true, compact: true, send_while_working: true, sub_agents: false, send_now: false }
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

    fn command(&self, name: &str, _arg: &str) -> Kind {
        match name {
            "model" => Kind::Local(Local::Model),
            "effort" => Kind::Local(Local::Effort),
            "compact" => Kind::Local(Local::Compact),
            "new" => Kind::Local(Local::New),
            "clear" => Kind::Unsupported,
            _ if TUI_ONLY.contains(&name) => Kind::Unsupported,
            _ => Kind::Send,
        }
    }

    fn builtins(&self) -> &'static [Builtin] {
        BUILTINS
    }

    fn pauses_goal(&self) -> bool {
        true
    }
}

/// pocketd turns the goal into `thread/goal/*` calls and the rest into a skill or a prompt.
const BUILTINS: &[Builtin] = &[
    Builtin { name: "goal", hint: "[<objective> | pause | resume | clear]", description: "Set a goal to keep working toward" },
    Builtin { name: "compact", hint: "", description: "Summarize the conversation to free up context" },
    Builtin { name: "model", hint: "[model]", description: "Set the model" },
    Builtin { name: "effort", hint: "[level]", description: "Set the reasoning effort" },
    Builtin { name: "new", hint: "", description: "Start a new chat" },
];

/// Codex's commands that only its TUI runs.
const TUI_ONLY: &[&str] = &[
    "approvals", "permissions", "experimental", "skills", "review", "rename", "resume", "fork", "init", "plan", "collab", "agent", "diff",
    "mention", "status", "statusline", "theme", "mcp", "apps", "logout", "quit", "exit", "feedback", "ps", "personality", "undo",
];
