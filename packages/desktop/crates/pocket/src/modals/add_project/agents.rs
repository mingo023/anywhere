use crate::desktop::Desktop;
use gpui_kit::*;
use std::collections::BTreeSet;

/// Whether the project's Agents switches show: not while only one agent is on offer, since a session needs it, unless
/// the project hides it.
fn agent_switches_shown(offered: &[&str], hidden: &BTreeSet<String>) -> bool {
    offered.len() > 1 || offered.iter().any(|p| hidden.contains(*p))
}

impl Desktop {
    /// One row per agent on offer, switched off where the project keeps it out of its model panel.
    pub(super) fn project_agent_rows(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let offered = self.offered();
        if !agent_switches_shown(&offered, &self.repo_form.draft.hidden) {
            return Vec::new();
        }
        offered.into_iter().map(|p| self.project_agent_row(p, cx).into_any_element()).collect()
    }

    fn project_agent_row(&self, provider: &'static str, cx: &mut Context<Self>) -> Div {
        let shown = !self.repo_form.draft.hidden.contains(provider);
        let toggle = div().id(SharedString::from(format!("repo-agent-{provider}"))).flex_none().cursor_pointer().child(ui::toggle("switch", shown)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
            let hidden = &mut this.repo_form.draft.hidden;
            if !hidden.remove(provider) {
                hidden.insert(provider.into());
            }
            cx.notify();
        }));
        super::agent_row(provider, self.agent_name(provider)).child(toggle)
    }
}

#[cfg(test)]
mod tests {
    use super::agent_switches_shown;
    use std::collections::BTreeSet;

    #[test]
    fn one_agent_on_offer_gets_no_switch_unless_the_project_hides_it() {
        let hidden = |ids: &[&str]| ids.iter().map(|p| p.to_string()).collect::<BTreeSet<_>>();
        assert!(!agent_switches_shown(&["claude"], &hidden(&[])));
        assert!(!agent_switches_shown(&["claude"], &hidden(&["codex"])), "hiding an agent not on offer");
        assert!(agent_switches_shown(&["claude"], &hidden(&["claude"])));
        assert!(agent_switches_shown(&["claude", "codex"], &hidden(&[])));
    }
}
