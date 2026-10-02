use crate::desktop::Desktop;
use crate::terminal_view::context;
use agents::{Agents, Level, level, percent};
use gpui_kit::*;
use theme::*;

/// Context left in each provider's newest session, and how full it is.
fn usage(agents: &Agents) -> Vec<(&'static str, u64, Level)> {
    let latest = |p: &str| agents.list.iter().filter(|a| a.provider == p).max_by_key(|a| a.updated_at)?.context();
    ["claude", "codex"].into_iter().filter_map(|p| latest(p).map(|(used, window)| (p, 100 - percent(used, window), level(used, window)))).collect()
}

impl Desktop {
    pub(super) fn usage(&self) -> Vec<(&'static str, u64, Level)> {
        usage(&self.agents)
    }

    pub(super) fn usage_card(&self) -> Option<Div> {
        let parts: Vec<Div> = self
            .usage()
            .into_iter()
            .map(|(p, left, level)| div().flex().items_center().gap(px(6.)).child(provider_icon(p, 12., TEXT_2)).child(div().text_color(context::text(level)).child(format!("{left}%"))))
            .collect();
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
                .child(div().ml_auto().text_color(TEXT_2).child("context left"))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::usage;
    use agents::{Agents, Level, Summary};

    fn session(agents: &mut Agents, id: &str, provider: &str, at: i64, used: u64) {
        agents.list.push(Summary { id: id.into(), provider: provider.into(), updated_at: at, tokens_used: used, context_window: Some(200_000), ..Default::default() });
    }

    #[test]
    fn usage_reads_the_context_left_in_each_providers_newest_session() {
        let mut agents = Agents::default();
        session(&mut agents, "codex", "codex", 1, 184_000);
        session(&mut agents, "old", "claude", 1, 20_000);
        session(&mut agents, "new", "claude", 2, 150_000);
        assert_eq!(usage(&agents), vec![("claude", 25, Level::Warn), ("codex", 8, Level::Danger)]);
    }

    #[test]
    fn usage_card_hides_without_a_window() {
        let mut agents = Agents::default();
        session(&mut agents, "old", "claude", 1, 20_000);
        agents.list.push(Summary { id: "new".into(), provider: "claude".into(), updated_at: 2, tokens_used: 20_000, ..Default::default() });
        assert_eq!(usage(&agents), vec![]);
    }
}
