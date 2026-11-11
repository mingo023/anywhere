use crate::desktop::Desktop;
use gpui_kit::*;

impl Desktop {
    /// One row per agent on offer, switched off where the project keeps it out of its model panel; none while only one
    /// agent is on offer, since a session needs it.
    pub(super) fn project_agent_rows(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let offered = self.offered();
        if offered.len() < 2 {
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
