use crate::adapter::{Backend, Capabilities};
use crate::{Ask, Fold, Transcript, Turn};

/// A provider this build has no adapter for: pocketd's items, without another agent's panels.
pub struct Other;

impl Backend for Other {
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
}
