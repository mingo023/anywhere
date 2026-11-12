use super::dropdown::{Choice, Row};
use crate::models::{by_upstream, effort_label, model_options, model_or_default};
use crate::pick_menu::grouped;
use agents::ListedModel;

/// `provider`'s models as a Settings menu lists them, after the agent's own default, so a pick can be undone.
pub(crate) fn model_rows(provider: &str, offered: &[ListedModel], current: &str) -> Vec<Row> {
    let choice = |value: &str, label: &str| Choice { value: value.into(), label: label.into() };
    let default = (!offered.iter().any(|m| m.value.is_empty())).then(|| Row::Pick(choice("", &model_or_default(""))));
    let groups = by_upstream(provider, model_options(offered, current).collect(), |(_, _, upstream)| upstream);
    default.into_iter().chain(grouped(groups.into_iter().map(|(who, models)| (who, models.into_iter().map(|(value, label, _)| choice(value, label)).collect())))).collect()
}

pub(crate) fn effort_options(efforts: &[String], none: &str) -> Vec<(String, String)> {
    std::iter::once((String::new(), none.to_string())).chain(efforts.iter().map(|e| (e.clone(), effort_label(e)))).collect()
}

/// "12 models available", as Settings says of an installed agent.
pub(crate) fn count_line(models: usize) -> String {
    format!("{models} {} available", if models == 1 { "model" } else { "models" })
}

#[cfg(test)]
mod tests {
    use super::{Choice, ListedModel, Row, count_line, effort_options, model_rows};
    use crate::models::Models;

    fn listed(value: &str, name: &str) -> ListedModel {
        ListedModel { value: value.into(), name: name.into(), resolved: value.into(), ..ListedModel::default() }
    }

    fn served(upstream: &str, value: &str, name: &str) -> ListedModel {
        ListedModel { upstream: upstream.into(), ..listed(value, name) }
    }

    #[test]
    fn the_agents_that_run_other_makers_models_list_them_under_who_serves_each() {
        let models = [served("anthropic", "anthropic/claude-opus-4-5", "Claude Opus 4.5"), served("amazon-bedrock", "amazon-bedrock/claude-opus-4-5", "Claude Opus 4.5"), served("anthropic", "anthropic/claude-haiku-4-5", "Claude Haiku 4.5")];
        let shown = |provider: &str, current: &str| {
            let rows = model_rows(provider, &models, current);
            rows.into_iter().map(|r| match r { Row::Heading(who) => format!("# {who}"), Row::Pick(Choice { label, .. }) => label }).collect::<Vec<_>>()
        };
        for provider in ["opencode", "pi", "omp", "hermes"] {
            assert_eq!(shown(provider, "")[1..], ["# ANTHROPIC", "Claude Opus 4.5", "Claude Haiku 4.5", "# AMAZON BEDROCK", "Claude Opus 4.5"], "{provider}");
        }
        assert_eq!(shown("cursor", "")[1..], ["Claude Opus 4.5", "Claude Opus 4.5", "Claude Haiku 4.5"]);
        assert_eq!(shown("pi", "local")[1..3], ["# PI", "local"], "a stored model that names nobody sits under the agent");
    }

    #[test]
    fn a_settings_model_menu_offers_the_agent_default_first_and_only_once() {
        let first = |offered: &[ListedModel]| match model_rows("claude", offered, "opus").into_iter().next() {
            Some(Row::Pick(Choice { value, label })) => (value, label),
            _ => panic!("no pick first"),
        };
        assert_eq!(first(&[listed("opus", "Opus")]), (String::new(), "Agent default".to_string()));
        let models = Models::default();
        let rows = model_rows("hermes", models.offered("hermes", true), "");
        assert_eq!(rows, [Row::Pick(Choice { value: String::new(), label: "Agent default".into() })], "an agent with no list already offers it");
    }

    #[test]
    fn an_installed_agent_counts_its_models() {
        assert_eq!((count_line(1), count_line(0), count_line(181)), ("1 model available".to_string(), "0 models available".to_string(), "181 models available".to_string()));
    }

    #[test]
    fn only_claude_code_offers_efforts() {
        assert_eq!(effort_options(&agents::providers::builtin_info("claude").efforts, "Agent default").iter().map(|(_, l)| l.as_str()).collect::<Vec<_>>(), ["Agent default", "Low", "Medium", "High", "Extra high", "Max"]);
        assert_eq!(effort_options(&[], "Agent default").len(), 1);
    }
}
