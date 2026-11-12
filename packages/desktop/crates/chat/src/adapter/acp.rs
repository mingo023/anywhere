use crate::adapter::{Backend, Capabilities};
use crate::commands::{Builtin, Kind, Local};
use crate::{Ask, Fold, Transcript, Turn};

/// An agent pocketd drives over the Agent Client Protocol: Cursor, Grok Build, OpenCode, fx, Hermes and Antigravity.
/// Each lists its own commands and takes them as prompts, so only what the chat itself picks stays local.
pub struct Acp {
    /// The agent's `providers::Builtin::tui_only`.
    pub tui_only: &'static [&'static str],
    /// The agent's `providers::Builtin::compacts`.
    pub compacts: bool,
}

impl Backend for Acp {
    type Item = agents::Item;
    type Ask = agents::Permission;

    fn capabilities(&self) -> Capabilities {
        Capabilities { interrupt: true, send_while_working: true, ..Default::default() }
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
            "new" => Kind::Local(Local::New),
            "compact" if self.compacts => Kind::Local(Local::Compact),
            _ if self.tui_only.contains(&name) => Kind::Unsupported,
            _ => Kind::Send,
        }
    }

    fn builtins(&self) -> &'static [Builtin] {
        if self.compacts { COMPACTING } else { BUILTINS }
    }
}

const MODEL: Builtin = Builtin { name: "model", hint: "[model]", description: "Set the model" };
const NEW: Builtin = Builtin { name: "new", hint: "", description: "Start a new chat" };
const BUILTINS: &[Builtin] = &[MODEL, NEW];
const COMPACTING: &[Builtin] = &[MODEL, Builtin { name: "compact", hint: "", description: "Summarize the conversation to free up context" }, NEW];
