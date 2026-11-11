use crate::adapter::{Backend, Capabilities};
use crate::commands::{Builtin, Kind, Local};
use crate::{Ask, Fold, Transcript, Turn};

/// Pi and omp, which pocketd drives over Pi's RPC mode.
pub struct Pi {
    /// The agent's `providers::Builtin::tui_only`.
    pub tui_only: &'static [&'static str],
}

impl Backend for Pi {
    type Item = agents::Item;
    type Ask = agents::Permission;

    fn capabilities(&self) -> Capabilities {
        Capabilities { interrupt: true, compact: true, send_while_working: true, ..Default::default() }
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
            "thinking" | "effort" => Kind::Local(Local::Effort),
            "compact" => Kind::Local(Local::Compact),
            "new" => Kind::Local(Local::New),
            _ if self.tui_only.contains(&name) => Kind::Unsupported,
            _ => Kind::Send,
        }
    }

    fn builtins(&self) -> &'static [Builtin] {
        BUILTINS
    }
}

const BUILTINS: &[Builtin] = &[
    Builtin { name: "model", hint: "[model]", description: "Set the model" },
    Builtin { name: "thinking", hint: "[level]", description: "Set the thinking level" },
    Builtin { name: "compact", hint: "", description: "Summarize the conversation to free up context" },
    Builtin { name: "new", hint: "", description: "Start a new chat" },
];
