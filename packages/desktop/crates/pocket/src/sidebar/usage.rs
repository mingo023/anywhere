use crate::desktop::Desktop;
use agents::Agents;
use gpui_kit::*;
use theme::*;
use ui::{self, dot};

/// Context left in each provider's newest open session.
fn usage(agents: &Agents) -> Vec<(&'static str, u64)> {
    let latest = |p: &str| {
        let a = agents.list.iter().filter(|a| a.provider == p && a.status != "closed").max_by_key(|a| a.updated_at)?;
        agents.context_left(&a.id)
    };
    ["claude", "codex"].into_iter().filter_map(|p| latest(p).map(|left| (p, left))).collect()
}

impl Desktop {
    pub(super) fn usage(&self) -> Vec<(&'static str, u64)> {
        usage(&self.agents)
    }

    pub(super) fn usage_card(&self) -> Option<Div> {
        let parts: Vec<Div> =
            self.usage().into_iter().map(|(p, left)| div().flex().items_center().gap(px(6.)).child(dot(7., provider_color(p))).child(format!("{left}%"))).collect();
        (!parts.is_empty()).then(|| {
            div()
                .mt(px(8.))
                .mx(px(2.))
                .py(px(8.))
                .px(px(10.))
                .flex()
                .items_center()
                .gap(px(12.))
                .rounded(px(12.))
                .bg(Token::new(0xffffff8c, 0xebebeb0d))
                .shadow(vec![ui::ring(FILL_3, 0.5)])
                .text_size(px(12.))
                .text_color(TEXT_2)
                .children(parts)
                .child(div().ml_auto().text_color(TEXT_4).child("context left"))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::usage;
    use agents::{Agents, Item, Summary, Usage};

    fn session(agents: &mut Agents, id: &str, provider: &str, status: &str, at: i64, used: u64) {
        agents.list.push(Summary { id: id.into(), provider: provider.into(), status: status.into(), updated_at: at, ..Default::default() });
        agents.timelines.insert(id.into(), vec![Item { usage: Some(Usage { input_tokens: used, cache_read_tokens: 0 }), ..Default::default() }]);
    }

    #[test]
    fn usage_reads_the_context_left_in_each_providers_newest_open_session() {
        let mut agents = Agents::default();
        session(&mut agents, "codex", "codex", "idle", 1, 100_000);
        session(&mut agents, "old", "claude", "working", 1, 20_000);
        session(&mut agents, "new", "claude", "idle", 2, 40_000);
        session(&mut agents, "closed", "claude", "closed", 3, 0);
        assert_eq!(usage(&agents), vec![("claude", 80), ("codex", 50)]);
    }

    #[test]
    fn usage_skips_a_provider_whose_newest_session_reported_none() {
        let mut agents = Agents::default();
        session(&mut agents, "old", "claude", "idle", 1, 20_000);
        agents.list.push(Summary { id: "new".into(), provider: "claude".into(), status: "working".into(), updated_at: 2, ..Default::default() });
        assert_eq!(usage(&agents), vec![]);
    }
}
