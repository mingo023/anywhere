use crate::adapter::{Backend, Capabilities};
use crate::commands::{Builtin, Kind, Local};
use crate::{Ask, Fold, Transcript, Turn};

pub struct Claude;

impl Backend for Claude {
    type Item = agents::Item;
    type Ask = agents::Permission;

    fn capabilities(&self) -> Capabilities {
        Capabilities { interrupt: true, compact: true, send_while_working: true, sub_agents: true, send_now: false }
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

    fn command(&self, name: &str, arg: &str) -> Kind {
        match name {
            "model" => Kind::Local(Local::Model),
            "effort" => Kind::Local(Local::Effort),
            "compact" if arg.is_empty() => Kind::Local(Local::Compact),
            "new" => Kind::Local(Local::New),
            "btw" => Kind::Local(Local::Btw),
            "mcp" => Kind::Local(Local::Mcp),
            "clear" | "reset" | "resume" => Kind::Unsupported,
            "add-dir" if arg.is_empty() => Kind::Unsupported,
            _ if PANELS.contains(&name) => Kind::Unsupported,
            _ => Kind::Send,
        }
    }

    fn builtins(&self) -> &'static [Builtin] {
        BUILTINS
    }
}

const BUILTINS: &[Builtin] = &[
    Builtin { name: "model", hint: "[model]", description: "Set the model" },
    Builtin { name: "effort", hint: "[level]", description: "Set the reasoning effort" },
    Builtin { name: "compact", hint: "[instructions]", description: "Summarize the conversation to free up context" },
    Builtin { name: "btw", hint: "[question]", description: "Ask a quick side question without interrupting" },
    Builtin { name: "goal", hint: "[<condition> | clear]", description: "Keep working until the condition is met" },
    Builtin { name: "mcp", hint: "", description: "Find an MCP server for this message" },
];

/// Claude's commands that open a panel, by name and alias.
const PANELS: &[&str] = &[
    "agents", "bug", "checkpoint", "checkup", "config", "cost", "diff", "doctor", "export", "feedback", "help", "hooks", "ide", "login", "logout",
    "memory", "permissions", "plugin", "privacy-settings", "rewind", "settings", "skills", "stats", "status", "tasks", "terminal-setup",
    "theme", "usage",
];
